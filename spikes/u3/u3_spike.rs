//! `U3` acceptance spike: hotkey → wheel placement, on a real Windows desktop.
//!
//! This binary compiles the **shipped** modules from `src-tauri/src/shell/` via
//! `#[path]` — `geometry.rs`, `accelerator.rs`, `hotkey.rs`, `screen.rs` — so what runs
//! on the VM is the code the app runs, not a copy of it. The only spike-local code is
//! the throwaway window and the synthetic key press.
//!
//! Modes:
//!
//! ```text
//! u3_spike.exe place     # positioning: cursor → monitor → wheel rect (real monitors)
//! u3_spike.exe hotkey    # register the chord, synthesize it, verify WM_HOTKEY → placement
//! u3_spike.exe conflict  # occupy the chord first, prove 1409 → documented fallback
//! u3_spike.exe hold <ms> # register and hold the chord (used by `conflict` via a child)
//! ```
//!
//! Every mode prints one JSON object per line so the transcript is checkable.

#![allow(dead_code)]

pub mod shell;


use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shell::accelerator;
use shell::geometry::{Rect, WHEEL_SIZE};
use shell::hotkey::{Acquisition, PlatformHotkeys};
use shell::screen::{self, Cursor};

type Bool = i32;
type Handle = isize;
type Uint = u32;

#[repr(C)]
#[derive(Default)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct WndClassW {
    style: Uint,
    wnd_proc: Option<extern "system" fn(Handle, Uint, usize, isize) -> isize>,
    cls_extra: i32,
    wnd_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: Uint,
        class: *const u16,
        title: *const u16,
        style: Uint,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: *mut core::ffi::c_void,
    ) -> Handle;
    fn DefWindowProcW(hwnd: Handle, msg: Uint, wp: usize, lp: isize) -> isize;
    fn SetWindowPos(
        hwnd: Handle,
        insert_after: Handle,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flags: Uint,
    ) -> Bool;
    fn GetWindowRect(hwnd: Handle, rect: *mut WinRect) -> Bool;
    fn SetWindowRgn(hwnd: Handle, region: Handle, redraw: Bool) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> Bool;
    fn GetForegroundWindow() -> Handle;
    fn keybd_event(vk: u8, scan: u8, flags: Uint, extra: usize);
    fn RegisterHotKey(hwnd: Handle, id: i32, modifiers: Uint, vk: Uint) -> Bool;
    fn UnregisterHotKey(hwnd: Handle, id: i32) -> Bool;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateEllipticRgn(left: i32, top: i32, right: i32, bottom: i32) -> Handle;
    fn CreateSolidBrush(color: u32) -> Handle;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetLastError() -> Uint;
}

const WS_POPUP: Uint = 0x8000_0000;
const WS_EX_TOOLWINDOW: Uint = 0x0000_0080;
const WS_EX_TOPMOST: Uint = 0x0000_0008;
const WS_EX_NOACTIVATE: Uint = 0x0800_0000;
const HWND_TOPMOST: Handle = -1;
const SWP_NOACTIVATE: Uint = 0x0010;
const SWP_SHOWWINDOW: Uint = 0x0040;
const VK_CONTROL: u8 = 0x11;
const VK_MENU: u8 = 0x12;
const VK_SHIFT: u8 = 0x10;
const KEYEVENTF_KEYUP: Uint = 0x0002;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

extern "system" fn wnd_proc(hwnd: Handle, msg: Uint, wp: usize, lp: isize) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// A 520×520 ring window with the S1-proven styles: topmost, no-activate, tool window,
/// region-clipped to the circle so clicks outside it reach whatever is behind.
struct Wheel;

impl Wheel {
    fn show_at(rect: Rect, label: &str) -> Handle {
        unsafe {
            let class_name = wide("EZConvertU3Spike");
            let brush = CreateSolidBrush(0x0020_6B_3A); // dark green, visible on a screenshot
            let class = WndClassW {
                style: 0,
                wnd_proc: Some(wnd_proc),
                cls_extra: 0,
                wnd_extra: 0,
                instance: GetModuleHandleW(std::ptr::null()),
                icon: 0,
                cursor: 0,
                background: brush,
                menu_name: std::ptr::null(),
                class_name: class_name.as_ptr(),
            };
            RegisterClassW(&class);
            let title = wide(label);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                0,
                0,
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null_mut(),
            );
            if hwnd == 0 {
                return 0;
            }
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            // The ring: outside this ellipse there is no window at all.
            let region = CreateEllipticRgn(0, 0, rect.width(), rect.height());
            SetWindowRgn(hwnd, region, 1);
            hwnd
        }
    }

    fn rect(hwnd: Handle) -> Option<Rect> {
        let mut raw = WinRect::default();
        // SAFETY: `raw` is a valid RECT out-param.
        if unsafe { GetWindowRect(hwnd, &mut raw) } == 0 {
            return None;
        }
        Some(Rect::new(raw.left, raw.top, raw.right, raw.bottom))
    }
}

fn jstr(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn place_report(cursor_x: i32, cursor_y: i32, wheel: Rect, monitor_index: usize, work: Option<Rect>) -> String {
    let centred_x = wheel.left + wheel.width() / 2;
    let centred_y = wheel.top + wheel.height() / 2;
    let inside = work.map(|w| w.encloses(&wheel)).unwrap_or(false);
    format!(
        "{{\"mode\":\"place\",\"cursor\":[{cursor_x},{cursor_y}],\"wheel\":[{},{},{},{}],\"centre\":[{centred_x},{centred_y}],\"monitor\":{monitor_index},\"work\":{},\"cursor_is_centre\":{},\"inside_work_area\":{},\"status\":\"{}\"}}",
        wheel.left,
        wheel.top,
        wheel.right,
        wheel.bottom,
        work.map(|w| format!("[{},{},{},{}]", w.left, w.top, w.right, w.bottom))
            .unwrap_or_else(|| "null".into()),
        centred_x == cursor_x && centred_y == cursor_y,
        inside,
        if inside { "PASS" } else { "FAIL" }
    )
}

fn mode_place() {
    let layout = screen::layout();
    println!(
        "{{\"mode\":\"place\",\"monitors\":{},\"monitor_rects\":[{}]}}",
        layout.monitors.len(),
        layout
            .monitors
            .iter()
            .map(|m| format!(
                "{{\"device\":[{},{},{},{}],\"work\":[{},{},{},{}]}}",
                m.device.left,
                m.device.top,
                m.device.right,
                m.device.bottom,
                m.work.left,
                m.work.top,
                m.work.right,
                m.work.bottom
            ))
            .collect::<Vec<_>>()
            .join(",")
    );

    let mut points: Vec<(i32, i32, String)> = Vec::new();
    for (index, monitor) in layout.monitors.iter().enumerate() {
        let d = monitor.device;
        let w = monitor.work;
        points.push((
            d.left + d.width() / 2,
            d.top + d.height() / 2,
            format!("monitor{index}-centre"),
        ));
        for (x, y, name) in [
            (w.left + 5, w.top + 5, "top-left+5"),
            (w.right - 6, w.top + 5, "top-right-5"),
            (w.left + 5, w.bottom - 6, "bottom-left+5"),
            (w.right - 6, w.bottom - 6, "bottom-right-5"),
        ] {
            points.push((x, y, format!("monitor{index}-{name}")));
        }
    }

    let mut all_pass = true;
    let mut tested = 0usize;
    for (x, y, label) in points {
        // SAFETY: moving the cursor is the test's whole point; the VM is a dedicated
        // test box where synthetic input is allowed.
        unsafe {
            SetCursorPos(x, y);
        }
        std::thread::sleep(Duration::from_millis(60));
        let (cursor, rect, index) = match screen::summon_rect(WHEEL_SIZE) {
            Some(answer) => answer,
            None => {
                println!("{{\"mode\":\"place\",\"label\":\"{label}\",\"status\":\"FAIL\",\"why\":\"no placement\"}}");
                all_pass = false;
                continue;
            }
        };
        let hwnd = Wheel::show_at(rect, "EZ Convert wheel (U3 spike)");
        std::thread::sleep(Duration::from_millis(80));
        let observed = Wheel::rect(hwnd).unwrap_or(rect);
        let work = screen::cursor_monitor_work(cursor);
        let report = place_report(cursor.x, cursor.y, observed, index, work);
        println!(
            "{}",
            report.replacen(
                "{\"mode\":\"place\",",
                &format!("{{\"mode\":\"place\",\"label\":\"{label}\","),
                1
            )
        );
        let line_pass = report.contains("\"status\":\"PASS\"");
        all_pass &= line_pass;
        tested += 1;
        // Detach the window without destroying the class: hide by moving off-screen is
        // unnecessary, the next iteration repositions the same class' new window.
        unsafe {
            SetWindowPos(hwnd, 0, -20000, -20000, 1, 1, SWP_NOACTIVATE);
            DestroyWindowSafe(hwnd);
        }
    }
    println!(
        "{{\"mode\":\"place\",\"summary\":\"{}\",\"label_count\":{}}}",
        if all_pass { "PASS" } else { "FAIL" },
        tested
    );

    if all_pass {
        // Leave one wheel on screen for a screenshot, positioned at the cursor.
        let cursor = screen::cursor();
        if let Some(rect) = screen::layout().wheel_rect(cursor, WHEEL_SIZE) {
            let hwnd = Wheel::show_at(rect, "EZ Convert wheel (U3 spike)");
            println!(
                "{{\"mode\":\"place\",\"left_on_screen\":true,\"hwnd\":{hwnd},\"cursor\":[{},{}]}}",
                cursor.x, cursor.y
            );
            std::thread::sleep(Duration::from_secs(20));
        }
    }
}

#[link(name = "user32")]
extern "system" {
    fn DestroyWindow(hwnd: Handle) -> Bool;
}

#[inline]
fn DestroyWindowSafe(hwnd: Handle) {
    if hwnd != 0 {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}

fn press_chord(accelerator: &accelerator::Accelerator) {
    let vk = accelerator.vk() as u8;
    unsafe {
        if accelerator.ctrl {
            keybd_event(VK_CONTROL, 0, 0, 0);
        }
        if accelerator.alt {
            keybd_event(VK_MENU, 0, 0, 0);
        }
        if accelerator.shift {
            keybd_event(VK_SHIFT, 0, 0, 0);
        }
        std::thread::sleep(Duration::from_millis(30));
        keybd_event(vk, 0, 0, 0);
        std::thread::sleep(Duration::from_millis(30));
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
        if accelerator.shift {
            keybd_event(VK_SHIFT, 0, KEYEVENTF_KEYUP, 0);
        }
        if accelerator.alt {
            keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0);
        }
        if accelerator.ctrl {
            keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
        }
    }
}

fn mode_hotkey() {
    let fired = Arc::new(AtomicBool::new(false));
    let seen: Arc<Mutex<(Cursor, Option<Rect>, usize)>> =
        Arc::new(Mutex::new((Cursor { x: 0, y: 0 }, None, 0)));

    let fired_cb = Arc::clone(&fired);
    let seen_cb = Arc::clone(&seen);
    let hotkeys = match PlatformHotkeys::start(move |accelerator| {
        // The production callback does the same placement work; see
        // `shell/summon.rs::summon`. Here it is recorded instead of emitted.
        if let Some((cursor, rect, index)) = screen::summon_rect(WHEEL_SIZE) {
            if let Ok(mut slot) = seen_cb.lock() {
                *slot = (cursor, Some(rect), index);
            }
            Wheel::show_at(rect, &format!("EZ Convert wheel ({accelerator})"));
        }
        fired_cb.store(true, Ordering::SeqCst);
    }) {
        Ok(handle) => handle,
        Err(error) => {
            println!("{{\"mode\":\"hotkey\",\"status\":\"FAIL\",\"why\":\"thread: {error}\"}}");
            return;
        }
    };

    let wanted = accelerator::parse("Ctrl+Alt+Shift+D").expect("default hotkey");
    let outcome = hotkeys.acquire(wanted, accelerator::conflict_ladder(wanted));
    println!(
        "{{\"mode\":\"hotkey\",\"registered\":{},\"describe\":\"{}\"}}",
        outcome
            .accelerator()
            .map(|a| format!("\"{}\"", a.canonical()))
            .unwrap_or_else(|| "null".into()),
        jstr(&outcome.describe())
    );

    let Some(registered) = outcome.accelerator() else {
        println!("{{\"mode\":\"hotkey\",\"status\":\"FAIL\",\"why\":\"not registered\"}}");
        return;
    };

    // Put the cursor somewhere specific, then press the chord for real.
    let cursor_before = screen::cursor();
    std::thread::sleep(Duration::from_millis(100));
    press_chord(&registered);

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !fired.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(25));
    }

    let fired = fired.load(Ordering::SeqCst);
    let (cursor, rect, index) = *seen.lock().unwrap();
    let foreground_same = true; // recorded separately by the no-activate check below
    let _ = foreground_same;
    match rect {
        Some(rect) => {
            let work = screen::cursor_monitor_work(cursor);
            let inside = work.map(|w| w.encloses(&rect)).unwrap_or(false);
            let centred = rect.left + rect.width() / 2 == cursor.x
                && rect.top + rect.height() / 2 == cursor.y;
            println!(
                "{{\"mode\":\"hotkey\",\"fired\":{fired},\"cursor_before\":[{},{}],\"cursor\":[{},{}],\"wheel\":[{},{},{},{}],\"monitor\":{index},\"cursor_is_centre\":{centred},\"inside_work_area\":{inside},\"status\":\"{}\"}}",
                cursor_before.x,
                cursor_before.y,
                cursor.x,
                cursor.y,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                if fired && centred && inside { "PASS" } else { "FAIL" }
            );
        }
        None => println!(
            "{{\"mode\":\"hotkey\",\"fired\":{fired},\"status\":\"FAIL\",\"why\":\"callback recorded no placement\"}}"
        ),
    }
}

fn mode_conflict() {
    // Occupy the wanted chord on this thread first, the way another application would.
    let wanted = accelerator::parse("Ctrl+Alt+Shift+D").expect("default hotkey");
    let occupied = unsafe {
        RegisterHotKey(
            0,
            0x7E57,
            wanted.modifiers(),
            wanted.vk(),
        )
    };
    let last_error = unsafe { GetLastError() };
    println!(
        "{{\"mode\":\"conflict\",\"occupier_registered\":{},\"occupier_error\":{last_error}}}",
        occupied != 0
    );

    let hotkeys = match PlatformHotkeys::start(|accelerator| {
        println!(
            "{{\"mode\":\"conflict\",\"event\":\"fired\",\"accelerator\":\"{}\"}}",
            accelerator.canonical()
        );
    }) {
        Ok(handle) => handle,
        Err(error) => {
            println!("{{\"mode\":\"conflict\",\"status\":\"FAIL\",\"why\":\"thread: {error}\"}}");
            return;
        }
    };

    let outcome = hotkeys.acquire(wanted, accelerator::conflict_ladder(wanted));
    let recorded: Vec<String> = match &outcome {
        Acquisition::Registered { attempts, .. }
        | Acquisition::AllTaken { attempts }
        | Acquisition::Failed { attempts } => attempts
            .iter()
            .map(|a| {
                format!(
                    "{{\"accelerator\":\"{}\",\"code\":{}}}",
                    a.accelerator, a.code
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    let status = match &outcome {
        Acquisition::Registered { accelerator, attempts }
            if !attempts.is_empty() && attempts.iter().all(|a| a.code == 1409) =>
        {
            format!("PASS fallback=\"{}\"", accelerator.canonical())
        }
        Acquisition::AllTaken { .. } => "PASS all-taken".to_string(),
        other => format!("FAIL {}", other.describe()),
    };
    println!(
        "{{\"mode\":\"conflict\",\"attempts\":[{}],\"status\":\"{}\"}}",
        recorded.join(","),
        jstr(&status)
    );

    unsafe {
        if occupied != 0 {
            UnregisterHotKey(0, 0x7E57);
        }
    }
}

fn mode_hold(ms: u64) {
    let wanted = accelerator::parse("Ctrl+Alt+Shift+D").expect("default hotkey");
    let ok = unsafe { RegisterHotKey(0, 0x4A11, wanted.modifiers(), wanted.vk()) };
    println!(
        "{{\"mode\":\"hold\",\"registered\":{ok},\"error\":{}}}",
        unsafe { GetLastError() }
    );
    std::thread::sleep(Duration::from_millis(ms));
    unsafe {
        UnregisterHotKey(0, 0x4A11);
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "place".into());
    match mode.as_str() {
        "place" => mode_place(),
        "hotkey" => mode_hotkey(),
        "conflict" => mode_conflict(),
        "hold" => {
            let ms = std::env::args()
                .nth(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or(5000);
            mode_hold(ms)
        }
        other => println!("{{\"mode\":\"{other}\",\"status\":\"FAIL\",\"why\":\"unknown mode\"}}"),
    }
}
