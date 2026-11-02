//! The wheel as a real overlay window (`SPEC.md` §1/§2, `ARCHITECTURE.md` §10.3/§10.4).
//!
//! Before this module existed `tauri.conf.json` declared exactly one window (`main`)
//! and `summon::summon` looked up a window labelled `wheel` that nothing ever created.
//! That lookup always returned `None`, so the position/show branch was dead code and
//! the hotkey, the tray item and the IPC command all produced no visible ring on any
//! platform. The overlay is therefore built **at runtime**, not declared in the
//! config, because every property that makes it an overlay is a decision this module
//! makes:
//!
//! - transparent, undecorated, `always_on_top`, off the taskbar, on every Space;
//! - **non-activating**: `show()` on macOS is `makeKeyAndOrderFront`, which would
//!   steal the key window from the app the user is working in. The window is created
//!   `focusable(false)`, so `show()` orders it front without making it key. A non-key
//!   window still receives mouse events, which is all the ring needs; the keyboard
//!   affordance (Esc) is watched system-wide instead of relying on focus;
//! - **click-through outside the ring**: on Windows the window is *shaped* with an
//!   elliptic `SetWindowRgn`, so clicks outside the disc are delivered to the window
//!   underneath by the window server itself. On macOS the same effect is produced by
//!   toggling `NSWindow.ignoresMouseEvents` from a 16 ms watcher that knows where the
//!   pointer is (the watcher also dismisses on Esc and on a click-away).
//!
//! The window is created at startup, hidden. Creating it on first summon would make
//! the first summon pay webview construction (hundreds of ms) and miss the 150 ms
//! budget the acceptance card measures.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::shell::geometry::{Rect, WHEEL_SIZE};

/// Window label the rest of the shell looks up.
pub const WHEEL_LABEL: &str = "wheel";
/// Emitted to the webview when the overlay is dismissed by the OS side (Esc,
/// click-away, a lost key window) so the ring can play its close animation.
pub const EVENT_DISMISS: &str = "wheel://dismiss";

/// How long the watcher sleeps between samples. 16 ms is one frame at 60 Hz: fast
/// enough that a click outside the disc is caught before the underlying app reacts,
/// cheap enough that an idle overlay costs nothing (the thread only runs while the
/// overlay is on screen).
const WATCH_INTERVAL: Duration = Duration::from_millis(16);

/// The rect the overlay currently occupies. Written by `present`, read by the
/// watcher; a static because the watcher must not depend on the webview still
/// existing to know where the wheel is.
static LAST_RECT: Mutex<Option<Rect>> = Mutex::new(None);
/// True while the dismiss watcher thread is alive.
static WATCHING: AtomicBool = AtomicBool::new(false);
/// Summon counter: a dismissal only hides the window the summon it belongs to
/// created, so a re-summon inside the close animation survives.
static SUMMONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How long the close animation is given before the window is hidden anyway.
const CLOSE_FALLBACK: Duration = Duration::from_millis(320);
/// `performance.now()`-style stamp: ms from the summon call to the webview's first
/// painted frame. `-1` means "no summon since boot".
static RENDER_MS: AtomicI64 = AtomicI64::new(-1);
static SUMMON_AT: Mutex<Option<Instant>> = Mutex::new(None);

// ---- pure rules (unit-tested on every host) ------------------------------

/// True when `point` is inside the disc inscribed in `rect`.
///
/// The wheel is a circle inside a square window; everything in the corners of that
/// square is material for the click-through rule. Denying a 1 px border would make
/// the rim un-clickable, so the disc is the full inscribed circle.
pub fn point_in_disc(point: (i32, i32), rect: Rect) -> bool {
    let width = rect.width().min(rect.height()) as f64;
    let radius = width / 2.0;
    let centre_x = rect.left as f64 + rect.width() as f64 / 2.0;
    let centre_y = rect.top as f64 + rect.height() as f64 / 2.0;
    let dx = point.0 as f64 - centre_x;
    let dy = point.1 as f64 - centre_y;
    dx * dx + dy * dy <= radius * radius
}

/// Everything the watcher knows, as data, so the dismissal rule is testable without
/// a display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchInput {
    pub visible: bool,
    pub focused: bool,
    pub escape_down: bool,
    pub primary_button_down: bool,
    pub pointer: (i32, i32),
    pub rect: Option<Rect>,
}

/// Why the overlay should dismiss, or `None` to stay up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismissal {
    Escape,
    ClickAway,
    LostFocus,
}

impl Dismissal {
    pub fn as_str(self) -> &'static str {
        match self {
            Dismissal::Escape => "escape",
            Dismissal::ClickAway => "click-away",
            Dismissal::LostFocus => "lost-focus",
        }
    }
}

/// The dismissal rule.
///
/// Esc is unconditional. A primary-button press is a click-away only **outside** the
/// disc — inside it the click belongs to a segment. A lost key window is *not* a
/// dismissal reason while the overlay is deliberately non-activating (it never had
/// the key window to lose), so focus is reported but never decisive: treating it as
/// one would close the wheel the instant it opened.
pub fn why_dismiss(input: &WatchInput) -> Option<Dismissal> {
    if !input.visible {
        return None;
    }
    if input.escape_down {
        return Some(Dismissal::Escape);
    }
    if input.primary_button_down {
        if let Some(rect) = input.rect {
            if !point_in_disc(input.pointer, rect) {
                return Some(Dismissal::ClickAway);
            }
        }
    }
    None
}

// ---- window lifecycle ----------------------------------------------------

fn overlay_url() -> WebviewUrl {
    WebviewUrl::App("wheel/index.html".into())
}

/// Position for the placement payload.
///
/// macOS reports screens in **points**, so a logical position keeps the 520 pt ring
/// at 520 pt on a Retina display; Windows reports physical pixels, so the payload is
/// already physical there. Mixing the two is how a 520 px wheel becomes a 260 px
/// wheel on a Mac.
fn apply_position(window: &WebviewWindow, payload: &crate::shell::summon::SummonPayload) {
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_position(tauri::LogicalPosition::new(
            payload.x as f64,
            payload.y as f64,
        ));
        let _ = window.set_size(tauri::LogicalSize::new(
            payload.width as f64,
            payload.height as f64,
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_position(tauri::PhysicalPosition::new(payload.x, payload.y));
        let _ = window.set_size(tauri::PhysicalSize::new(
            payload.width as u32,
            payload.height as u32,
        ));
    }
}

/// Creates the overlay window once. Idempotent: a second call returns the window the
/// first one built.
pub fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(WHEEL_LABEL) {
        return Some(window);
    }
    let (width, height) = WHEEL_SIZE;
    let built = WebviewWindowBuilder::new(app, WHEEL_LABEL, overlay_url())
        .title("EZ Convert wheel")
        .inner_size(width as f64, height as f64)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        // A full-screen app on another Space would otherwise hide the wheel.
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        // The ring is a mouse surface, not a document: the first click selects a
        // segment instead of merely activating the window.
        .accept_first_mouse(true)
        // Not focusable, on purpose — see the module comment.
        .focusable(false)
        .focused(false)
        .visible(false)
        // A hidden webview that is throttled stops running JS and would miss the
        // summon event on the first press.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .build();
    match built {
        Ok(window) => {
            shape_window(&window);
            Some(window)
        }
        Err(error) => {
            eprintln!("wheel overlay: cannot create the wheel window: {error}");
            None
        }
    }
}

/// Builds the overlay at startup so the first summon is a position + show + paint,
/// not a webview construction.
pub fn install(app: &AppHandle) {
    if ensure(app).is_none() {
        return;
    }
    // The watcher only runs while the overlay is visible; `present` starts it.
    eprintln!("wheel overlay: window created (hidden)");
}

/// Records the summon instant, so the webview's render acknowledgement can be turned
/// into the millisecond figure the acceptance card asks for.
pub fn mark_summon() -> Instant {
    let now = Instant::now();
    if let Ok(mut slot) = SUMMON_AT.lock() {
        *slot = Some(now);
    }
    RENDER_MS.store(-1, Ordering::SeqCst);
    now
}

/// Called by the webview once the summoned ring has painted.
pub fn note_rendered() {
    let elapsed = SUMMON_AT
        .lock()
        .ok()
        .and_then(|slot| *slot)
        .map(|at| at.elapsed().as_millis() as i64)
        .unwrap_or(-1);
    RENDER_MS.store(elapsed, Ordering::SeqCst);
}

/// Millisecond figure for the last summon (`-1` when the webview never answered).
pub fn rendered_ms() -> i64 {
    RENDER_MS.load(Ordering::SeqCst)
}

/// Places and shows the overlay for a placement decision.
pub fn present(app: &AppHandle, payload: &crate::shell::summon::SummonPayload) -> Option<WebviewWindow> {
    SUMMONS.fetch_add(1, Ordering::SeqCst);
    let window = ensure(app)?;
    if payload.placed {
        apply_position(&window, payload);
    }
    *LAST_RECT.lock().ok()? = Some(Rect::from_origin(
        payload.x,
        payload.y,
        payload.width,
        payload.height,
    ));
    shape_window(&window);
    // `focusable(false)` + `show()` = order front without becoming key.
    let _ = window.show();
    start_watcher(app.clone());
    Some(window)
}

/// Hides the overlay and tells the webview to play its close animation.
///
/// The window is not hidden here: the webview closes with the spring and calls
/// `overlay_closed`. The timer is the fallback for a webview that cannot answer (and
/// it is cancelled implicitly by a newer summon, so a fast re-summon is never hidden
/// by the previous dismissal).
pub fn dismiss(app: &AppHandle, reason: Dismissal) {
    WATCHING.store(false, Ordering::SeqCst);
    let _ = app.emit(EVENT_DISMISS, reason.as_str());
    let generation = SUMMONS.load(Ordering::SeqCst);
    let forward = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CLOSE_FALLBACK);
        if SUMMONS.load(Ordering::SeqCst) == generation {
            hide(&forward);
        }
    });
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WHEEL_LABEL) {
        let _ = window.hide();
    }
}

// ---- the watcher ---------------------------------------------------------

fn start_watcher(app: AppHandle) {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let mut pointer_was_inside = true;
        while WATCHING.load(Ordering::SeqCst) {
            std::thread::sleep(WATCH_INTERVAL);
            let Some(window) = app.get_webview_window(WHEEL_LABEL) else {
                break;
            };
            let visible = window.is_visible().unwrap_or(false);
            let focused = window.is_focused().unwrap_or(false);
            let rect = LAST_RECT.lock().ok().and_then(|slot| *slot);
            let cursor = crate::shell::screen::cursor();
            let input = WatchInput {
                visible,
                focused,
                escape_down: crate::shell::screen::escape_down(),
                primary_button_down: crate::shell::screen::primary_button_down(),
                pointer: (cursor.x, cursor.y),
                rect,
            };
            if let Some(reason) = why_dismiss(&input) {
                dismiss(&app, reason);
                break;
            }
            // Click-through: the disc is interactive, the corners of the window are
            // not. Windows does this with a window region (see `shape_window`), so
            // only macOS needs the toggle here.
            #[cfg(target_os = "macos")]
            if let Some(rect) = rect {
                let inside = point_in_disc(input.pointer, rect);
                if inside != pointer_was_inside {
                    let _ = window.set_ignore_cursor_events(!inside);
                    pointer_was_inside = inside;
                }
            }
            #[cfg(not(target_os = "macos"))]
            let _ = &mut pointer_was_inside;
        }
        WATCHING.store(false, Ordering::SeqCst);
    });
}

/// Shapes the window to the ring's disc (Windows) so the corners are *not ours* at
/// the window-server level — the strongest form of click-through, and one a
/// screenshot and `GetWindowRgn` can both prove.
fn shape_window(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        if let Ok(hwnd) = window.hwnd() {
            if !win32::clip_to_disc(hwnd.0 as isize, WHEEL_SIZE) {
                eprintln!("wheel overlay: cannot shape the window to the ring");
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = window;
    }
}

/// Read back what the window server thinks the overlay's shape is, for the
/// measurement probe: `Some(region_type)` on Windows (`3` = `COMPLEXREGION`, which is
/// what an ellipse reports), `None` where the platform has no such notion.
pub fn region_type(window: &WebviewWindow) -> Option<i32> {
    #[cfg(windows)]
    {
        window
            .hwnd()
            .ok()
            .and_then(|hwnd| win32::region_type(hwnd.0 as isize))
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        None
    }
}

/// Read back `NSWindow.ignoresMouseEvents`, the macOS click-through flag.
pub fn ignores_mouse_events(window: &WebviewWindow) -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWindow;
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        let sent = window.with_webview(move |webview| {
            let pointer = webview.ns_window();
            // SAFETY: `ns_window()` is the NSWindow Tauri created for this webview and
            // `with_webview` runs the closure on the main thread, where AppKit may be
            // touched.
            let ns = unsafe { &*(pointer as *const NSWindow) };
            let _ = tx.send(unsafe { ns.ignoresMouseEvents() });
        });
        if sent.is_err() {
            return None;
        }
        rx.recv_timeout(Duration::from_millis(500)).ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        None
    }
}

/// Windows: is the **top-level** window under `(x, y)` this overlay?
///
/// With the elliptic region in place the corners of the square window are not ours,
/// so `WindowFromPoint` there answers with the window underneath. That is the
/// click-through property, asked of the window server instead of the app.
pub fn window_at_point_is_ours(window: &WebviewWindow, x: i32, y: i32) -> Option<bool> {
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().ok()?;
        let found = win32::root_window_at(x, y);
        if found == 0 {
            return None;
        }
        Some(found == hwnd.0 as isize)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, x, y);
        None
    }
}

// ---- platform shaping primitives (hand-rolled FFI, as in `screen.rs`) ----

#[cfg(windows)]
mod win32 {
    type Handle = isize;
    type Bool = i32;

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateEllipticRgn(left: i32, top: i32, right: i32, bottom: i32) -> Handle;
        fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32) -> Handle;
        fn DeleteObject(object: Handle) -> Bool;
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowRgn(window: Handle, region: Handle, redraw: Bool) -> i32;
        fn GetWindowRgn(window: Handle, region: Handle) -> i32;
        fn WindowFromPoint(point: Point) -> Handle;
        fn GetAncestor(window: Handle, flags: u32) -> Handle;
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    /// `GA_ROOT`.
    const GA_ROOT: u32 = 2;

    /// The top-level window under `(x, y)` — `WindowFromPoint` answers with a child
    /// (WebView2 owns several), so the result is walked to its root.
    pub fn root_window_at(x: i32, y: i32) -> Handle {
        // SAFETY: plain value call.
        let child = unsafe { WindowFromPoint(Point { x, y }) };
        if child == 0 {
            return 0;
        }
        // SAFETY: `child` is a live window handle the window server returned.
        unsafe { GetAncestor(child, GA_ROOT) }
    }

    /// `SetWindowRgn` takes ownership of the region on success, so the region must be
    /// deleted only when the call failed (otherwise it leaks, or worse, is
    /// double-freed).
    pub fn clip_to_disc(hwnd: Handle, size: (i32, i32)) -> bool {
        // SAFETY: plain value call.
        let region = unsafe { CreateEllipticRgn(0, 0, size.0, size.1) };
        if region == 0 {
            return false;
        }
        // SAFETY: `hwnd` came from the window handle Tauri owns; `region` is a valid
        // HRGN.
        let ok = unsafe { SetWindowRgn(hwnd, region, 1) } != 0;
        if !ok {
            // SAFETY: the system did not take ownership.
            unsafe { DeleteObject(region) };
        }
        ok
    }

    /// The region type the window server reports for `hwnd`, or `None` when the
    /// window has no region. Ellipse -> `COMPLEXREGION` (3).
    pub fn region_type(hwnd: Handle) -> Option<i32> {
        // SAFETY: a scratch region of our own.
        let scratch = unsafe { CreateRectRgn(0, 0, 1, 1) };
        if scratch == 0 {
            return None;
        }
        // SAFETY: valid window handle and our own scratch region.
        let kind = unsafe { GetWindowRgn(hwnd, scratch) };
        // SAFETY: `GetWindowRgn` never takes ownership.
        unsafe { DeleteObject(scratch) };
        if kind <= 0 {
            None
        } else {
            Some(kind)
        }
    }
}

// ---- IPC surface ---------------------------------------------------------

/// The webview calls this on boot so a summon that arrived before the listener was
/// registered is not lost: the answer is the current placement (or `None`).
#[tauri::command]
pub fn overlay_ready() -> Option<crate::shell::summon::SummonPayload> {
    LAST_RECT.lock().ok().and_then(|slot| *slot).map(|rect| {
        crate::shell::summon::SummonPayload {
            x: rect.left,
            y: rect.top,
            width: rect.width(),
            height: rect.height(),
            cursor_x: 0,
            cursor_y: 0,
            monitor_index: 0,
            placed: true,
            grabbed: Vec::new(),
            grab_note: None,
        }
    })
}

/// The webview's first painted frame after a summon.
#[tauri::command]
pub fn overlay_rendered() {
    note_rendered();
}

/// Esc / click-outside handled inside the webview.
#[tauri::command]
pub fn overlay_dismiss(app: AppHandle) {
    dismiss(&app, Dismissal::Escape);
}

/// The webview finished its close animation: hide the window now instead of waiting
/// for the fallback timer.
#[tauri::command]
pub fn overlay_closed(app: AppHandle) {
    hide(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel_rect() -> Rect {
        Rect::from_origin(100, 100, 520, 520)
    }

    #[test]
    fn the_centre_and_the_rim_are_inside_the_disc() {
        let rect = wheel_rect();
        assert!(point_in_disc((360, 360), rect), "centre");
        assert!(point_in_disc((360, 100), rect), "top of the rim");
        assert!(point_in_disc((360, 620), rect), "bottom of the rim");
        assert!(point_in_disc((100, 360), rect), "left of the rim");
    }

    #[test]
    fn the_corners_are_outside_the_disc() {
        let rect = wheel_rect();
        for corner in [(101, 101), (619, 101), (101, 619), (619, 619)] {
            assert!(
                !point_in_disc(corner, rect),
                "{corner:?} is in the window but outside the ring"
            );
        }
    }

    fn watching(over: impl FnOnce(&mut WatchInput)) -> WatchInput {
        let mut input = WatchInput {
            visible: true,
            focused: false,
            escape_down: false,
            primary_button_down: false,
            pointer: (360, 360),
            rect: Some(wheel_rect()),
        };
        over(&mut input);
        input
    }

    #[test]
    fn escape_dismisses_from_anywhere() {
        let input = watching(|i| {
            i.escape_down = true;
            i.pointer = (0, 0);
        });
        assert_eq!(why_dismiss(&input), Some(Dismissal::Escape));
    }

    #[test]
    fn a_click_inside_the_disc_is_a_segment_not_a_dismissal() {
        let input = watching(|i| i.primary_button_down = true);
        assert_eq!(why_dismiss(&input), None);
    }

    #[test]
    fn a_click_outside_the_disc_dismisses() {
        let input = watching(|i| {
            i.primary_button_down = true;
            i.pointer = (1000, 1000);
        });
        assert_eq!(why_dismiss(&input), Some(Dismissal::ClickAway));
    }

    #[test]
    fn a_hidden_overlay_never_dismisses_itself() {
        let input = watching(|i| {
            i.visible = false;
            i.escape_down = true;
        });
        assert_eq!(why_dismiss(&input), None);
    }

    #[test]
    fn never_having_the_key_window_is_not_a_dismissal() {
        // The overlay is non-activating by design: `focused == false` is its normal
        // state and must not close it on the first frame.
        let input = watching(|i| i.focused = false);
        assert_eq!(why_dismiss(&input), None);
    }
}
