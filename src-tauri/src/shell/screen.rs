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
}

#[cfg(windows)]
pub use win32::{cursor, cursor_monitor_work, layout};

#[cfg(not(windows))]
mod other {
    use super::*;

    /// PLANNED: macOS uses `NSEvent::mouseLocation` + `NSScreen::screens`. Both are
    /// unmeasured on this project (`ARCHITECTURE.md` §10.5), so this returns the
    /// empty/None shapes rather than an invented geometry. A caller that sees `None`
    /// must treat the summon as failed, not as "place at 0,0".
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
}

#[cfg(not(windows))]
pub use other::{cursor, cursor_monitor_work, layout};

/// The whole summon decision in one call: cursor → layout → wheel rect. Returns the
/// rect plus the monitor index, or `None` when the platform cannot answer.
pub fn summon_rect(size: (i32, i32)) -> Option<(Cursor, Rect, usize)> {
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
}
