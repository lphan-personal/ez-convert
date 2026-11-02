//! `--overlay-probe` — the measurement harness for the overlay acceptance card.
//!
//! The card's acceptance criteria are statements about a *window on a real desktop*:
//! it exists, it is at the cursor, it keeps the ring click-through outside the disc,
//! it never takes the key window, and it paints inside 150 ms. None of those can be
//! proved by a unit test, so the app itself measures them and writes the result as
//! JSON — the same code path the hotkey and the tray take (`summon::summon`), run
//! from a worker thread exactly like the hotkey callback does.
//!
//! Usage:
//!
//! ```text
//! ez-convert --overlay-probe <facts.json> [--overlay-hold <seconds>]
//! ```
//!
//! The probe writes the JSON, touches `<facts.json>.ready` so the calling script can
//! take a screenshot while the ring is still on screen, holds the window for
//! `--overlay-hold` seconds (default 6) and exits 0. It exits non-zero when a
//! required check failed, so CI fails on the measurement rather than on a grep of it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::shell::geometry::Rect;
use crate::shell::screen;

/// Where the facts are written, when the binary was started as a probe.
pub fn requested() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|arg| arg == "--overlay-probe") {
        return None;
    }
    Some(
        arg_value(&args, "--overlay-probe")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("overlay-facts.json")),
    )
}

fn hold_seconds() -> u64 {
    let args: Vec<String> = std::env::args().collect();
    arg_value(&args, "--overlay-hold")
        .and_then(|value| value.parse().ok())
        .unwrap_or(6)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[derive(Debug, Serialize)]
struct MonitorFacts {
    device: [i32; 4],
    work: [i32; 4],
}

#[derive(Debug, Serialize)]
struct WindowFacts {
    found: bool,
    visible: bool,
    focused: bool,
    always_on_top: bool,
    decorated: bool,
    /// Physical pixels, as the platform reports them.
    outer_position: Option<[i32; 2]>,
    outer_size: Option<[u32; 2]>,
    scale_factor: Option<f64>,
    /// What the window server thinks the click-through shape is.
    region_type: Option<i32>,
    ignores_mouse_events: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ClickSamples {
    /// Pointer warped to the disc centre.
    centre: [i32; 2],
    centre_cursor: [i32; 2],
    centre_inside_disc: bool,
    centre_ignores_mouse_events: Option<bool>,
    /// Pointer warped into the window but outside the disc (a corner).
    corner: [i32; 2],
    corner_cursor: [i32; 2],
    corner_inside_disc: bool,
    corner_ignores_mouse_events: Option<bool>,
    /// Windows: the top-level window under the corner, for comparison with ours.
    corner_window_is_ours: Option<bool>,
    warped: bool,
}

#[derive(Debug, Serialize)]
struct Facts {
    os: &'static str,
    arch: &'static str,
    probe: &'static str,
    monitors: Vec<MonitorFacts>,
    cursor_before: [i32; 2],
    placement: PlacementFacts,
    window: WindowFacts,
    click: ClickSamples,
    main_focused_before: bool,
    main_focused_after: bool,
    /// ms from the summon call to the webview's first painted frame.
    render_ms: i64,
    checks: BTreeMap<String, bool>,
    asserted: Vec<String>,
    ok: bool,
}

#[derive(Debug, Serialize)]
struct PlacementFacts {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    cursor_x: i32,
    cursor_y: i32,
    monitor_index: usize,
    placed: bool,
    grab_note: Option<String>,
    /// True when the placement payload's rect spans the cursor it reported.
    cursor_inside_rect: bool,
    /// True when the monitor index is a real index into the reported layout.
    monitor_in_range: bool,
}

fn rect_array(rect: Rect) -> [i32; 4] {
    [rect.left, rect.top, rect.right, rect.bottom]
}

/// Runs a geometry query on the main thread (AppKit's screen list is
/// `MainThreadOnly`; the probe thread is not the main thread).
fn on_main<T: Send + 'static>(app: &AppHandle, job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(job());
    })
    .ok()?;
    rx.recv_timeout(Duration::from_millis(2000)).ok()
}

fn measure(app: &AppHandle) -> Facts {
    let main = app.get_webview_window("main");
    let main_focused_before = main
        .as_ref()
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    let cursor_before = on_main(app, screen::cursor).unwrap_or(screen::Cursor { x: 0, y: 0 });

    // The measured path: the same function the global hotkey and the tray call, from
    // a worker thread — the hotkey thread's situation.
    let payload = crate::shell::summon::summon(app);

    // Wait for the webview's first painted frame (bounded: a broken webview must
    // produce a recorded failure, not a hung CI job).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while crate::shell::overlay::rendered_ms() < 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let render_ms = crate::shell::overlay::rendered_ms();

    // Let the frame settle before sampling window state.
    std::thread::sleep(Duration::from_millis(200));

    let window = app.get_webview_window(crate::shell::overlay::WHEEL_LABEL);
    let monitors = on_main(app, screen::layout)
        .map(|layout| {
            layout
                .monitors
                .iter()
                .map(|monitor| MonitorFacts {
                    device: rect_array(monitor.device),
                    work: rect_array(monitor.work),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let window_facts = match window.as_ref() {
        Some(window) => WindowFacts {
            found: true,
            visible: window.is_visible().unwrap_or(false),
            focused: window.is_focused().unwrap_or(false),
            always_on_top: window.is_always_on_top().unwrap_or(false),
            decorated: window.is_decorated().unwrap_or(true),
            outer_position: window
                .outer_position()
                .ok()
                .map(|position| [position.x, position.y]),
            outer_size: window.outer_size().ok().map(|size| [size.width, size.height]),
            scale_factor: window.scale_factor().ok(),
            region_type: crate::shell::overlay::region_type(window),
            ignores_mouse_events: crate::shell::overlay::ignores_mouse_events(window),
        },
        None => WindowFacts {
            found: false,
            visible: false,
            focused: false,
            always_on_top: false,
            decorated: true,
            outer_position: None,
            outer_size: None,
            scale_factor: None,
            region_type: None,
            ignores_mouse_events: None,
        },
    };

    let main_focused_after = main
        .as_ref()
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);

    // ---- click-through samples ------------------------------------------
    let rect = Rect::from_origin(payload.x, payload.y, payload.width, payload.height);
    let centre = (rect.left + rect.width() / 2, rect.top + rect.height() / 2);
    let corner = (rect.left + 6, rect.top + 6);

    let mut warped = screen::warp_pointer(centre.0, centre.1);
    std::thread::sleep(Duration::from_millis(150));
    let centre_cursor = on_main(app, screen::cursor).unwrap_or(screen::Cursor { x: 0, y: 0 });
    let centre_inside = crate::shell::overlay::point_in_disc((centre_cursor.x, centre_cursor.y), rect);
    let centre_ignores = window
        .as_ref()
        .and_then(crate::shell::overlay::ignores_mouse_events);

    warped = screen::warp_pointer(corner.0, corner.1) && warped;
    std::thread::sleep(Duration::from_millis(150));
    let corner_cursor = on_main(app, screen::cursor).unwrap_or(screen::Cursor { x: 0, y: 0 });
    let corner_inside = crate::shell::overlay::point_in_disc((corner_cursor.x, corner_cursor.y), rect);
    let corner_ignores = window
        .as_ref()
        .and_then(crate::shell::overlay::ignores_mouse_events);
    let corner_window_is_ours = window
        .as_ref()
        .and_then(|window| {
            crate::shell::overlay::window_at_point_is_ours(window, corner.0, corner.1)
        });

    // Leave the pointer where it was: a probe must not move the user's cursor.
    let _ = screen::warp_pointer(cursor_before.x, cursor_before.y);

    // ---- checks ---------------------------------------------------------
    let mut checks: BTreeMap<String, bool> = BTreeMap::new();
    checks.insert("window-created".into(), window_facts.found);
    checks.insert("window-visible".into(), window_facts.visible);
    checks.insert("placement-reported".into(), payload.placed);
    checks.insert(
        "position-matches-placement".into(),
        position_matches(&window_facts, &payload),
    );
    checks.insert("size-matches-wheel".into(), size_matches(&window_facts, &payload));
    checks.insert("always-on-top".into(), window_facts.always_on_top);
    checks.insert("undecorated".into(), !window_facts.decorated);
    checks.insert("overlay-not-key".into(), !window_facts.focused);
    checks.insert(
        "main-window-keeps-focus".into(),
        main_focused_before == main_focused_after,
    );
    checks.insert(
        "rendered-within-150ms".into(),
        render_ms >= 0 && render_ms <= 150,
    );
    checks.insert(
        "cursor-inside-wheel-rect".into(),
        rect.contains(payload.cursor_x, payload.cursor_y),
    );
    checks.insert(
        "monitor-index-in-range".into(),
        payload.monitor_index < monitors.len(),
    );
    checks.insert("pointer-warps".into(), warped);

    // Which of the two click-through mechanisms the platform actually has. Each is
    // asserted only where it is implemented and observable.
    #[cfg(windows)]
    {
        checks.insert("window-region-is-ellipse".into(), window_facts.region_type == Some(3));
        checks.insert("corner-belongs-to-another-window".into(), corner_window_is_ours == Some(false));
    }
    #[cfg(target_os = "macos")]
    {
        checks.insert(
            "disc-accepts-mouse-events".into(),
            centre_ignores == Some(false),
        );
        checks.insert(
            "corner-ignores-mouse-events".into(),
            corner_ignores == Some(true),
        );
    }

    // The checks this platform can prove. Everything in `checks` is recorded; only
    // these decide the exit code — a check whose mechanism does not exist on the
    // platform (the Windows region on macOS, say) is not reported as a pass either.
    #[cfg(windows)]
    let asserted: Vec<String> = vec![
        "window-created",
        "window-visible",
        "placement-reported",
        "position-matches-placement",
        "size-matches-wheel",
        "always-on-top",
        "undecorated",
        "overlay-not-key",
        "main-window-keeps-focus",
        "rendered-within-150ms",
        "cursor-inside-wheel-rect",
        "monitor-index-in-range",
        "pointer-warps",
        "window-region-is-ellipse",
        "corner-belongs-to-another-window",
    ]
    .into_iter()
    .map(|name| name.to_string())
    .collect();
    #[cfg(target_os = "macos")]
    let asserted: Vec<String> = vec![
        "window-created",
        "window-visible",
        "placement-reported",
        "position-matches-placement",
        "size-matches-wheel",
        "always-on-top",
        "undecorated",
        "overlay-not-key",
        "main-window-keeps-focus",
        "rendered-within-150ms",
        "cursor-inside-wheel-rect",
        "monitor-index-in-range",
        "pointer-warps",
        "disc-accepts-mouse-events",
        "corner-ignores-mouse-events",
    ]
    .into_iter()
    .map(|name| name.to_string())
    .collect();
    #[cfg(not(any(windows, target_os = "macos")))]
    let asserted: Vec<String> = vec!["placement-reported".to_string()];

    let ok = asserted
        .iter()
        .all(|name| checks.get(name).copied().unwrap_or(false));

    Facts {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        probe: "overlay",
        monitors,
        cursor_before: [cursor_before.x, cursor_before.y],
        placement: PlacementFacts {
            x: payload.x,
            y: payload.y,
            width: payload.width,
            height: payload.height,
            cursor_x: payload.cursor_x,
            cursor_y: payload.cursor_y,
            monitor_index: payload.monitor_index,
            placed: payload.placed,
            grab_note: payload.grab_note.clone(),
            cursor_inside_rect: rect.contains(payload.cursor_x, payload.cursor_y),
            monitor_in_range: payload.monitor_index < monitors.len(),
        },
        window: window_facts,
        click: ClickSamples {
            centre: [centre.0, centre.1],
            centre_cursor: [centre_cursor.x, centre_cursor.y],
            centre_inside_disc: centre_inside,
            centre_ignores_mouse_events: centre_ignores,
            corner: [corner.0, corner.1],
            corner_cursor: [corner_cursor.x, corner_cursor.y],
            corner_inside_disc: corner_inside,
            corner_ignores_mouse_events: corner_ignores,
            corner_window_is_ours,
            warped,
        },
        main_focused_before,
        main_focused_after,
        render_ms,
        checks,
        asserted,
        ok,
    }
}

/// Does the platform's reported window position agree with the placement, in the
/// units that platform uses? macOS reports physical pixels for a logical placement,
/// so the comparison divides by the scale factor; Windows' payload is already
/// physical.
fn position_matches(window: &WindowFacts, payload: &crate::shell::summon::SummonPayload) -> bool {
    let Some(position) = window.outer_position else {
        return false;
    };
    let scale = window.scale_factor.unwrap_or(1.0);
    #[cfg(target_os = "macos")]
    let expected = (
        (payload.x as f64 * scale).round() as i32,
        (payload.y as f64 * scale).round() as i32,
    );
    #[cfg(not(target_os = "macos"))]
    let expected = (payload.x, payload.y);
    // A window server is allowed to be a pixel or two off on a scaled display; more
    // than that is a placement bug, not rounding.
    (position[0] - expected.0).abs() <= 2 && (position[1] - expected.1).abs() <= 2
}

fn size_matches(window: &WindowFacts, payload: &crate::shell::summon::SummonPayload) -> bool {
    let Some(size) = window.outer_size else {
        return false;
    };
    let scale = window.scale_factor.unwrap_or(1.0);
    #[cfg(target_os = "macos")]
    let expected = (
        (payload.width as f64 * scale).round() as i32,
        (payload.height as f64 * scale).round() as i32,
    );
    #[cfg(not(target_os = "macos"))]
    let expected = (payload.width as i32, payload.height as i32);
    (size[0] as i32 - expected.0).abs() <= 2 && (size[1] as i32 - expected.1).abs() <= 2
}

/// Runs the probe on its own thread, writes the facts, holds the overlay for the
/// screenshot, then exits with the measurement's verdict.
pub fn run(app: AppHandle, out: PathBuf) {
    std::thread::spawn(move || {
        // The hidden overlay webview is created in `.setup()`; give it time to load
        // so the first summon does not measure webview construction.
        std::thread::sleep(Duration::from_millis(1500));
        let facts = measure(&app);
        let json = serde_json::to_string_pretty(&facts).unwrap_or_else(|error| {
            format!("{{\"ok\":false,\"error\":\"cannot serialise facts: {error}\"}}")
        });
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&out, &json) {
            Ok(()) => {
                println!("{json}");
                // Exactly `<facts>.ready`: that is the file `scripts/overlay-probe.sh`
                // waits for before it photographs the ring.
                let mut marker = out.clone().into_os_string();
                marker.push(".ready");
                let _ = std::fs::write(PathBuf::from(marker), b"ready\n");
            }
            Err(error) => eprintln!("overlay probe: cannot write {}: {error}", out.display()),
        }
        // Keep the ring on screen so the harness can photograph it.
        std::thread::sleep(Duration::from_secs(hold_seconds()));
        app.exit(if facts.ok { 0 } else { 1 });
    });
}
