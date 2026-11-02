//! The cursor, the monitor layout, and the region-clip primitive
//! (`ARCHITECTURE.md` §10.1-§10.2).
//!
//! Deliberately hand-rolled FFI rather than a `windows`/`windows-sys` dependency:
//! this module is compiled unchanged into the `U3` acceptance spike
//! (`spikes/u3/u3_spike.rs`) and into the app, so the thing measured on the VM is
//! the thing that ships. Every binding below is `extern "system"` against user32,
//! so no crate feature set can change its behaviour between the two builds.
//!
//! `geometry.rs` owns the placement *rule*; this module only answers "where is the
//! cursor" and "what monitors exist", which needs the OS.

use crate::shell::geometry::{Monitor, Rect};

/// Where the pointer is, in virtual-desktop coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
}

/// The live monitor layout: every monitor's device rect + work area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub monitors: Vec<Monitor>,
}

impl Layout {
    /// The rect the wheel should occupy for a cursor position, or `None` when the OS
    /// reports no monitors (a session-0 or headless case that must never panic).
    pub fn wheel_rect(&self, cursor: Cursor, size: (i32, i32)) -> Option<Rect> {
        crate::shell::geometry::place_on_layout(&self.monitors, (cursor.x, cursor.y), size)
    }

    /// Index of the monitor the cursor is on, using the same nearest-monitor rule as
    /// placement. Diagnostics use it to name the monitor in a log line.
    pub fn monitor_of(&self, cursor: Cursor) -> Option<usize> {
        crate::shell::geometry::monitor_at(&self.monitors, cursor.x, cursor.y)
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;

    type Bool = i32;
    type Handle = isize;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct WinRect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct MonitorInfoW {
        cb_size: u32,
        rc_monitor: WinRect,
        rc_work: WinRect,
        flags: u32,
    }

    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(point: *mut Point) -> Bool;
        fn SetCursorPos(x: i32, y: i32) -> Bool;
        /// High bit set == the key is down now. Used by the overlay's dismiss
        /// watcher for Esc and the primary button, so dismissal needs no hook and
        /// no permission.
        fn GetAsyncKeyState(virtual_key: i32) -> i16;
        fn GetSystemMetrics(index: i32) -> i32;
        fn MonitorFromPoint(point: Point, flags: u32) -> Handle;
        fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfoW) -> Bool;
        fn EnumDisplayMonitors(
            dc: Handle,
            clip: *const WinRect,
            callback: extern "system" fn(Handle, Handle, *mut WinRect, isize) -> Bool,
            data: isize,
        ) -> Bool;
    }

    /// `SM_XVIRTUALSCREEN`… `SM_CYVIRTUALSCREEN`: the bounding box of every monitor,
    /// used only as a fallback when enumeration returns nothing.
    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;

    extern "system" fn collect(monitor: Handle, _dc: Handle, _clip: *mut WinRect, data: isize) -> Bool {
        // SAFETY: `data` is the `&mut Vec<Monitor>` passed to `EnumDisplayMonitors`
        // below, which outlives the enumeration.
        let sink = unsafe { &mut *(data as *mut Vec<Monitor>) };
        let mut info = MonitorInfoW {
            cb_size: std::mem::size_of::<MonitorInfoW>() as u32,
            rc_monitor: WinRect::default(),
            rc_work: WinRect::default(),
            flags: 0,
        };
        // SAFETY: `monitor` is a valid HMONITOR handed to us by the system and
        // `info` is a correctly sized MONITORINFO.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            sink.push(Monitor::new(
                Rect::new(
                    info.rc_monitor.left,
                    info.rc_monitor.top,
                    info.rc_monitor.right,
                    info.rc_monitor.bottom,
                ),
                Rect::new(
                    info.rc_work.left,
                    info.rc_work.top,
                    info.rc_work.right,
                    info.rc_work.bottom,
                ),
            ));
        }
        1
    }

    pub fn cursor() -> Cursor {
        let mut point = Point::default();
        // SAFETY: `point` is a valid, writable POINT and the call has no other
        // precondition.
        if unsafe { GetCursorPos(&mut point) } != 0 {
            Cursor {
                x: point.x,
                y: point.y,
            }
        } else {
            Cursor { x: 0, y: 0 }
        }
    }

    pub fn layout() -> Layout {
        let mut monitors: Vec<Monitor> = Vec::new();
        // SAFETY: the callback appends to `monitors`, which lives until the call
        // returns; `EnumDisplayMonitors` is synchronous.
        let ok = unsafe {
            EnumDisplayMonitors(0, std::ptr::null(), collect, &mut monitors as *mut _ as isize)
        };
        if ok == 0 || monitors.is_empty() {
            // Fallback: one synthetic monitor covering the virtual screen. This is
            // exactly the spike's old rule, kept only so a headless call still
            // returns something usable — and it is *never* preferred when the OS
            // reports a real layout.
            let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
            let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
            let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
            let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
            monitors.push(Monitor::plain(x, y, x + w, y + h));
        }
        Layout { monitors }
    }

    /// The work area of the monitor the cursor is on: `MonitorFromPoint` +
    /// `GetMonitorInfoW`, the exact pair `ARCHITECTURE.md` §10.2 specifies.
    pub fn cursor_monitor_work(cursor: Cursor) -> Option<Rect> {
        let point = Point {
            x: cursor.x,
            y: cursor.y,
        };
        // SAFETY: plain value call.
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        if monitor == 0 {
            return None;
        }
        let mut info = MonitorInfoW {
            cb_size: std::mem::size_of::<MonitorInfoW>() as u32,
            rc_monitor: WinRect::default(),
            rc_work: WinRect::default(),
            flags: 0,
        };
        // SAFETY: valid HMONITOR from the system, correctly sized struct.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return None;
        }
        Some(Rect::new(
            info.rc_work.left,
            info.rc_work.top,
            info.rc_work.right,
            info.rc_work.bottom,
        ))
    }

    const VK_LBUTTON: i32 = 0x01;
    const VK_ESCAPE: i32 = 0x1B;

    /// `GetAsyncKeyState` reports the state as it is *now*, with no hook and no
    /// permission — the overlay's dismiss watcher polls it while it is on screen.
    pub fn key_down(virtual_key: i32) -> bool {
        // SAFETY: plain value call; a key code is always in range.
        (unsafe { GetAsyncKeyState(virtual_key) } as u16) & 0x8000 != 0
    }

    pub fn escape_down() -> bool {
        key_down(VK_ESCAPE)
    }

    pub fn primary_button_down() -> bool {
        key_down(VK_LBUTTON)
    }

    /// Moves the pointer. Only the measurement probe calls it (to sample the
    /// click-through boundary at a known point); the app never warps the cursor.
    pub fn warp_pointer(x: i32, y: i32) -> bool {
        // SAFETY: plain value call.
        (unsafe { SetCursorPos(x, y) }) != 0
    }
}

// ---- Cocoa ↔ virtual-desktop mathematics ---------------------------------
//
// AppKit reports geometry in **Cocoa screen coordinates**: the origin is the
// bottom-left corner of the *primary* screen and y grows **upward**. Every other
// module here (and `tauri::PhysicalPosition`) uses the Win32 convention: the origin
// is the top-left corner of the primary screen and y grows **downward**. The
// conversion is a reflection about the primary screen's top edge, which is exactly
// what makes negative-origin monitors (a screen left of, or above, the primary) come
// out right without a special case:
//
//     y_virtual = reference_max_y - y_cocoa - height_cocoa
//
// `reference_max_y` is the primary screen's `frame.maxY` (its height, because the
// primary's Cocoa origin is (0, 0)). Kept as pure functions so the flip is unit
// tested on every host, not only on a Mac.

/// A Cocoa rect (origin bottom-left of the primary, y up) as a virtual-desktop rect.
pub fn cocoa_rect_to_virtual(
    reference_max_y: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Rect {
    Rect::new(
        x.round() as i32,
        (reference_max_y - y - height).round() as i32,
        (x + width).round() as i32,
        (reference_max_y - y).round() as i32,
    )
}

/// A Cocoa point as a virtual-desktop point. `NSEvent.mouseLocation` is in Cocoa
/// coordinates, so this is the call that stops "the wheel is upside down on the
/// second monitor" from shipping.
pub fn cocoa_point_to_virtual(reference_max_y: f64, x: f64, y: f64) -> (i32, i32) {
    (x.round() as i32, (reference_max_y - y).round() as i32)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::NSRect;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        /// Key state straight from the HID system: no event tap, no Accessibility
        /// grant. 53 is `kVK_Escape`.
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
        fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    /// `kCGEventSourceStateHIDSystemState`.
    const HID_SYSTEM_STATE: i32 = 1;
    /// `kVK_Escape`.
    const VK_ESCAPE_MAC: u16 = 53;

    /// `NSScreen` is `MainThreadOnly`: an off-thread call is a programming error, so
    /// this is the guard the caller checks before marshalling (`shell::summon`).
    pub fn on_main_thread() -> bool {
        MainThreadMarker::new().is_some()
    }

    /// The primaries' reflection line plus every screen as a monitor. `None` off the
    /// main thread (AppKit is not safe there) so the caller can say so instead of
    /// inventing a geometry.
    fn screens() -> Option<(f64, Vec<Monitor>)> {
        let marker = MainThreadMarker::new()?;
        let list = NSScreen::screens(marker);
        let mut pairs: Vec<(NSRect, NSRect)> = Vec::new();
        for screen in list.iter() {
            pairs.push((screen.frame(), screen.visibleFrame()));
        }
        // The primary screen is the one whose Cocoa origin is (0, 0); its `maxY` is
        // the reflection line. Falling back to the first screen keeps a headless
        // session from producing a nonsensical flip.
        let reference = pairs
            .iter()
            .find(|(frame, _)| frame.origin.x.abs() < 0.5 && frame.origin.y.abs() < 0.5)
            .or_else(|| pairs.first())
            .map(|(frame, _)| frame.origin.y + frame.size.height)
            .unwrap_or(0.0);
        let monitors = pairs
            .iter()
            .map(|(frame, visible)| {
                Monitor::new(
                    cocoa_rect_to_virtual(
                        reference,
                        frame.origin.x,
                        frame.origin.y,
                        frame.size.width,
                        frame.size.height,
                    ),
                    cocoa_rect_to_virtual(
                        reference,
                        visible.origin.x,
                        visible.origin.y,
                        visible.size.width,
                        visible.size.height,
                    ),
                )
            })
            .collect();
        Some((reference, monitors))
    }

    pub fn cursor() -> Cursor {
        let Some((reference, _)) = screens() else {
            return Cursor { x: 0, y: 0 };
        };
        let point = NSEvent::mouseLocation();
        let (x, y) = cocoa_point_to_virtual(reference, point.x, point.y);
        Cursor { x, y }
    }

    pub fn layout() -> Layout {
        match screens() {
            Some((_, monitors)) => Layout { monitors },
            None => Layout {
                monitors: Vec::new(),
            },
        }
    }

    /// The work area of the monitor the cursor is on, by the same nearest-monitor
    /// rule the placement uses (`NSScreen.visibleFrame` is already the work area:
    /// menu bar and Dock excluded).
    pub fn cursor_monitor_work(cursor: Cursor) -> Option<Rect> {
        let layout = layout();
        let index = layout.monitor_of(cursor)?;
        Some(layout.monitors[index].work)
    }

    pub fn escape_down() -> bool {
        // SAFETY: plain value call.
        unsafe { CGEventSourceKeyState(HID_SYSTEM_STATE, VK_ESCAPE_MAC) }
    }

    pub fn primary_button_down() -> bool {
        // Bit 0 of `pressedMouseButtons` is the left button.
        NSEvent::pressedMouseButtons() & 1 != 0
    }

    /// Only the measurement probe warps the pointer (to sample the click-through
    /// boundary at a known point); the app never does.
    pub fn warp_pointer(x: i32, y: i32) -> bool {
        let Some((reference, _)) = screens() else {
            return false;
        };
        // Warping takes Cocoa coordinates, so the conversion runs backwards.
        let point = CGPoint {
            x: x as f64,
            y: reference - y as f64,
        };
        // SAFETY: plain value call; a point outside every display is clamped by the
        // window server, never a fault.
        (unsafe { CGWarpMouseCursorPosition(point) }) == 0
    }
}

/// Non-Windows, non-macOS hosts (the Linux CI and dev containers) answer nothing.
/// A caller that sees `None` must treat the summon as failed, not as "place at 0,0".
#[cfg(not(any(windows, target_os = "macos")))]
mod other {
    use super::*;

    pub fn cursor() -> Cursor {
        Cursor { x: 0, y: 0 }
    }

    pub fn cursor_monitor_work(_cursor: Cursor) -> Option<Rect> {
        None
    }

    pub fn layout() -> Layout {
        Layout {
            monitors: Vec::new(),
        }
    }

    pub fn escape_down() -> bool {
        false
    }

    pub fn primary_button_down() -> bool {
        false
    }

    pub fn warp_pointer(_x: i32, _y: i32) -> bool {
        false
    }
}

#[cfg(windows)]
pub use win32::{
    cursor, cursor_monitor_work, escape_down, layout, primary_button_down, warp_pointer,
};

#[cfg(target_os = "macos")]
pub use macos::{cursor, cursor_monitor_work, escape_down, layout, primary_button_down, warp_pointer};

#[cfg(not(any(windows, target_os = "macos")))]
pub use other::{cursor, cursor_monitor_work, escape_down, layout, primary_button_down, warp_pointer};

/// Why a platform answered nothing, for the payload's `grab_note`. A summon that is
/// not placed must say which of the two reasons it was.
pub fn unavailable_reason() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if !macos::on_main_thread() {
            return "the macOS screen list is main-thread only and this call was not marshalled";
        }
    }
    "this platform cannot report the cursor or its monitors"
}

/// True when the caller is on a thread that may ask AppKit for screen geometry.
/// Non-macOS platforms have no such restriction.
pub fn can_query_geometry() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::on_main_thread()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// The whole summon decision in one call: cursor → layout → wheel rect. Returns the
/// rect plus the monitor index, or `None` when the platform cannot answer.
pub fn summon_rect(size: (i32, i32)) -> Option<(Cursor, Rect, usize)> {
    if !can_query_geometry() {
        return None;
    }
    let cursor = cursor();
    let layout = layout();
    let rect = layout.wheel_rect(cursor, size)?;
    let index = layout.monitor_of(cursor)?;
    Some((cursor, rect, index))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summon_rect_never_returns_a_rect_without_a_monitor() {
        // On a runner with no display this returns None; on Windows it returns a rect
        // inside a real work area. Either is acceptable — a panic or a (0,0) rect with
        // no monitor is not.
        if let Some((cursor, rect, index)) = summon_rect(crate::shell::geometry::WHEEL_SIZE) {
            let layout = layout();
            assert!(index < layout.monitors.len());
            assert!(layout.monitors[index].work.encloses(&rect));
            assert!((rect.left..rect.right).contains(&cursor.x));
            assert!((rect.top..rect.bottom).contains(&cursor.y));
        }
    }

    // ---- Cocoa → virtual-desktop conversion -------------------------------
    //
    // These run on every host because the maths is pure: the flip is the part that
    // used to be "PLANNED: unmeasured" and it must not depend on having a Mac.

    /// 1920×1080 primary, a second screen to its left (negative x) and a third
    /// above it (negative y once flipped) — the two cases a naive flip gets wrong.
    const REFERENCE: f64 = 1080.0;

    #[test]
    fn the_primary_screen_maps_onto_itself() {
        let rect = cocoa_rect_to_virtual(REFERENCE, 0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(rect, Rect::new(0, 0, 1920, 1080));
    }

    #[test]
    fn a_screen_to_the_left_keeps_negative_x_and_lands_at_the_same_top() {
        let rect = cocoa_rect_to_virtual(REFERENCE, -1920.0, 0.0, 1920.0, 1080.0);
        assert_eq!(rect, Rect::new(-1920, 0, 0, 1080));
    }

    #[test]
    fn a_screen_above_the_primary_becomes_negative_y() {
        // Cocoa: sitting on top of the primary means a positive y origin.
        let rect = cocoa_rect_to_virtual(REFERENCE, 0.0, 1080.0, 1920.0, 1080.0);
        assert_eq!(rect, Rect::new(0, -1080, 1920, 0));
    }

    #[test]
    fn a_screen_below_the_primary_becomes_a_positive_y() {
        let rect = cocoa_rect_to_virtual(REFERENCE, 0.0, -1080.0, 1920.0, 1080.0);
        assert_eq!(rect, Rect::new(0, 1080, 1920, 2160));
    }

    #[test]
    fn the_visible_frame_is_inset_from_the_top_by_the_menu_bar() {
        // A 25 pt menu bar at the top of the primary screen.
        let work = cocoa_rect_to_virtual(REFERENCE, 0.0, 0.0, 1920.0, 1055.0);
        assert_eq!(work, Rect::new(0, 25, 1920, 1080));
    }

    #[test]
    fn a_cocoa_point_flips_about_the_primaries_top_edge() {
        // 200 pt above the bottom of the primary = 880 px below its top.
        assert_eq!(cocoa_point_to_virtual(REFERENCE, 100.0, 200.0), (100, 880));
        // The very top-left corner of the primary.
        assert_eq!(cocoa_point_to_virtual(REFERENCE, 0.0, 1080.0), (0, 0));
        // One row above the primary's top edge (a screen placed above it).
        assert_eq!(cocoa_point_to_virtual(REFERENCE, 0.0, 1081.0), (0, -1));
        // Into the screen to the left: x is unchanged, y still flips.
        assert_eq!(cocoa_point_to_virtual(REFERENCE, -100.0, 540.0), (-100, 540));
    }

    #[test]
    fn a_flipped_cursor_and_a_flipped_screen_agree_on_the_monitor() {
        // The end-to-end shape of the macOS bug: flip the screens, flip the cursor,
        // then run the ordinary placement rule.
        let primary = cocoa_rect_to_virtual(REFERENCE, 0.0, 0.0, 1920.0, 1080.0);
        let left = cocoa_rect_to_virtual(REFERENCE, -1920.0, 0.0, 1920.0, 1080.0);
        let monitors = vec![Monitor::plain(primary.left, primary.top, primary.right, primary.bottom),
                            Monitor::plain(left.left, left.top, left.right, left.bottom)];
        let (cx, cy) = cocoa_point_to_virtual(REFERENCE, -100.0, 540.0);
        assert_eq!(crate::shell::geometry::monitor_at(&monitors, cx, cy), Some(1));
        let wheel = crate::shell::geometry::place_on_layout(
            &monitors,
            (cx, cy),
            crate::shell::geometry::WHEEL_SIZE,
        )
        .expect("layout");
        assert!(wheel.right <= 0, "wheel {wheel:?} crossed onto the primary");
    }
}
