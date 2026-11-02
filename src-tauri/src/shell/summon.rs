//! Summoning the wheel, and the smart-grab toggle (`SPEC.md` §2).
//!
//! This is the one place where "the hotkey fired" becomes a wall-clock event: it reads
//! the cursor, asks `screen` for the monitor layout, places the wheel with the
//! `geometry` rule, and hands the result to the webview. Nothing about the wheel's
//! *content* lives here — that is `U1`.
//!
//! Every step is also exposed as a Tauri command so the surface can be exercised
//! without a registered global hotkey (a menu click and a hotkey must take the same
//! path, or the acceptance test measures something the user never does).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::shell::geometry::WHEEL_SIZE;
use crate::shell::grab::{GrabError, PermissionState, SmartGrab, SystemPermission};
use crate::shell::hotkey::Acquisition;
use crate::shell::ipc::AppState;

/// Tauri event emitted on every summon. The wheel window listens for it; the payload
/// is the placement decision, so the webview never has to compute geometry.
pub const EVENT_SUMMON: &str = "wheel://summon";
/// Tauri event emitted when smart grab produced paths (or refused, with a reason).
pub const EVENT_GRAB: &str = "wheel://grab";

/// What the webview receives on `wheel://summon`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummonPayload {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Cursor in virtual-desktop coordinates (may be negative on a multi-monitor
    /// desktop — that is the whole point of `geometry::place_on_layout`).
    pub cursor_x: i32,
    pub cursor_y: i32,
    /// Index into the OS monitor list, for diagnostics.
    pub monitor_index: usize,
    /// `false` when the platform could not answer (macOS in v1); the wheel must not
    /// be shown at a made-up position.
    pub placed: bool,
    /// Set when smart grab ran because nothing was being dragged.
    pub grabbed: Vec<String>,
    /// Why smart grab did not run, if it did not.
    pub grab_note: Option<String>,
}

/// Places the wheel for the cursor and tells the wheel window where to be.
///
/// Done in one function so the tray item, the global hotkey and the IPC command
/// cannot diverge. Returns the payload (also emitted as an event) for the caller to
/// log or assert on.
pub fn summon(app: &AppHandle) -> SummonPayload {
    // Stamped before the placement query so the millisecond figure the acceptance
    // card asks for covers everything the user waits for, not just the paint.
    crate::shell::overlay::mark_summon();
    let payload = placement(app);

    // Creates the window if it is somehow gone, then positions, shows without taking
    // the key window, and starts the dismiss watcher. Until this module existed the
    // lookup here was the only thing between a hotkey press and a silent no-op.
    crate::shell::overlay::present(app, &payload);

    let _ = app.emit(EVENT_SUMMON, &payload);
    payload
}

/// The macOS screen list from the right thread.
///
/// `NSScreen` is `MainThreadOnly`, while the global hotkey fires on its own thread.
/// Off the main thread the query is marshalled and waited for (a few hundred µs); on
/// the main thread it runs directly — waiting there would deadlock, and the
/// measurement probe calls this from the main thread.
fn summon_rect(
    app: &AppHandle,
    size: (i32, i32),
) -> Option<(
    crate::shell::screen::Cursor,
    crate::shell::geometry::Rect,
    usize,
)> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::mpsc;
        use std::time::Duration;
        if crate::shell::screen::can_query_geometry() {
            return crate::shell::screen::summon_rect(size);
        }
        let (tx, rx) = mpsc::channel();
        if app
            .run_on_main_thread(move || {
                let _ = tx.send(crate::shell::screen::summon_rect(size));
            })
            .is_err()
        {
            return None;
        }
        return rx.recv_timeout(Duration::from_millis(500)).ok().flatten();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        crate::shell::screen::summon_rect(size)
    }
}

/// The placement decision on its own — no window, no event. This is the half the
/// acceptance spike asserts against.
pub fn placement(app: &AppHandle) -> SummonPayload {
    let (cursor, rect, monitor_index) = match summon_rect(app, WHEEL_SIZE) {
        Some(answer) => answer,
        None => {
            return SummonPayload {
                x: 0,
                y: 0,
                width: WHEEL_SIZE.0,
                height: WHEEL_SIZE.1,
                cursor_x: 0,
                cursor_y: 0,
                monitor_index: 0,
                placed: false,
                grabbed: Vec::new(),
                grab_note: Some(crate::shell::screen::unavailable_reason().into()),
            }
        }
    };

    let (grabbed, grab_note) = smart_grab_paths(app);

    SummonPayload {
        x: rect.left,
        y: rect.top,
        width: rect.width(),
        height: rect.height(),
        cursor_x: cursor.x,
        cursor_y: cursor.y,
        monitor_index,
        placed: true,
        grabbed,
        grab_note,
    }
}

/// Runs smart grab if (and only if) the user turned it on.
///
/// The refusal path is deliberate: `SmartGrab::grab` returns `Disabled` without
/// touching the selection source, so a hotkey press with the toggle off never reads
/// Explorer/Finder and never asks the OS anything (`SPEC.md` §2).
fn smart_grab_paths(app: &AppHandle) -> (Vec<String>, Option<String>) {
    let Some(state) = app.try_state::<AppState>() else {
        return (Vec::new(), Some("app state unavailable".into()));
    };
    let grab = state.smart_grab_now();
    if !grab.enabled() {
        return (Vec::new(), None);
    }
    let mut source = crate::shell::grab::platform_selection_source();
    match grab.grab(source.as_mut()) {
        Ok(paths) => (
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            None,
        ),
        Err(GrabError::Disabled) => (Vec::new(), None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

/// The tray item / settings toggle. Enabling shows the permission flow; disabling
/// touches no permission API (`ARCHITECTURE.md` §15).
pub fn toggle_smart_grab(app: &AppHandle) -> serde_json::Value {
    let enabled_now = app
        .try_state::<AppState>()
        .map(|state| state.smart_grab_now().enabled())
        .unwrap_or(false);
    set_smart_grab(app, !enabled_now)
}

/// Enables/disables smart grab, persists the result, and keeps the tray check mark and
/// the settings panel in step.
pub fn set_smart_grab(app: &AppHandle, enabled: bool) -> serde_json::Value {
    let outcome = match app.try_state::<AppState>() {
        Some(state) => state.set_smart_grab(enabled),
        None => return serde_json::json!({ "error": "app state unavailable" }),
    };
    crate::shell::tray::sync_smart_grab(app, outcome.enabled);
    // Persist `smart_grab.enabled` + the reported permission state through the same
    // store the settings panel writes, so a restart cannot disagree with the menu.
    if let Some(state) = app.try_state::<AppState>() {
        state.persist_smart_grab(outcome.enabled, outcome.permission);
    }
    serde_json::json!({
        "enabled": outcome.enabled,
        "permission": outcome.permission.as_str(),
        "prompted": outcome.prompted,
        "message": outcome.message,
    })
}

/// Brings the main window forward (menu "Settings…").
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The hotkey's current registration, for the settings panel and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyView {
    pub configured: String,
    pub registered: Option<String>,
    pub status: String,
}

pub fn hotkey_snapshot(state: &AppState) -> HotkeyView {
    let configured = state.hotkey_setting();
    let acquisition = state.hotkey_status();
    HotkeyView {
        configured,
        registered: acquisition.as_ref().and_then(|a| a.accelerator()).map(|a| a.canonical()),
        status: acquisition
            .as_ref()
            .map(|a| a.describe())
            .unwrap_or_else(|| "not started".into()),
    }
}

/// Re-registers the hotkey after the user edits the accelerator.
pub fn rebind_hotkey(app: &AppHandle, accelerator: &str) -> HotkeyView {
    let Some(state) = app.try_state::<AppState>() else {
        return HotkeyView {
            configured: accelerator.to_string(),
            registered: None,
            status: "app state unavailable".into(),
        };
    };
    let acquisition: Acquisition = state.rebind_hotkey(accelerator);
    HotkeyView {
        configured: accelerator.to_string(),
        registered: acquisition.accelerator().map(|a| a.canonical()),
        status: acquisition.describe(),
    }
}

/// The shared smart-grab cell, re-exported so `ipc` and this module cannot each grow
/// their own copy of the permission state.
pub type SmartGrabCell = Mutex<SmartGrab>;

/// The permission probe used by production builds.
pub fn system_permission() -> SystemPermission {
    SystemPermission
}

/// The state a fresh app starts with.
pub fn initial_smart_grab(enabled: bool, permission: &str) -> SmartGrab {
    let state = PermissionState::from_setting(permission);
    // Persisted "enabled + denied" is a contradiction; the permission wins.
    SmartGrab::new(enabled && state.allows_grab(), state)
}

/// Starts the hotkey thread and registers whatever `settings.json` asks for.
///
/// Called once from `.setup()`. A failure is never fatal: the app opens, the tray
/// works, and the settings panel reports why the chord is not registered — a launch
/// that dies because a hotkey is taken would be a worse product than one that says so.
pub fn start_hotkeys(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let forward = app.clone();
    let hotkeys = match crate::shell::hotkey::PlatformHotkeys::start(move |_accelerator| {
        // Runs on the hotkey thread: keep it cheap. `summon` emits an event and
        // repositions the wheel window; the webview does the rest.
        let _ = summon(&forward);
    }) {
        Ok(hotkeys) => hotkeys,
        Err(error) => {
            state.note_hotkey_failure(&format!("hotkey thread failed to start: {error}"));
            return;
        }
    };
    state.install_hotkey_thread(hotkeys);
    let configured = state.hotkey_setting();
    let outcome = state.rebind_hotkey(&configured);
    eprintln!("{}", outcome.describe());
}

// ---- IPC surface (U3) -------------------------------------------------------
//
// The wheel window and the settings panel read state through these; they exist so the
// menu, the hotkey and a manual summon all take the same path.

/// Places the wheel at the cursor and emits `wheel://summon`.
#[tauri::command]
pub fn wheel_summon(app: AppHandle) -> SummonPayload {
    summon(&app)
}

/// The placement decision with no side effects — what the acceptance test asserts on.
#[tauri::command]
pub fn wheel_placement(app: AppHandle) -> SummonPayload {
    placement(&app)
}

/// Current hotkey registration (configured vs actually registered, plus the reason).
#[tauri::command]
pub fn hotkey_view(state: tauri::State<'_, AppState>) -> HotkeyView {
    hotkey_snapshot(&state)
}

/// Re-registers the hotkey after the settings panel edits `hotkey.accelerator`.
#[tauri::command]
pub fn hotkey_rebind(app: AppHandle, accelerator: String) -> HotkeyView {
    rebind_hotkey(&app, &accelerator)
}

/// Turns smart grab on/off. Enabling is the only call in the app that can prompt.
#[tauri::command]
pub fn smart_grab_set(app: AppHandle, enabled: bool) -> serde_json::Value {
    set_smart_grab(&app, enabled)
}

/// Smart grab's current state and permission, for the settings panel.
#[tauri::command]
pub fn smart_grab_view(state: tauri::State<'_, AppState>) -> serde_json::Value {
    let grab = state.smart_grab_now();
    serde_json::json!({
        "enabled": grab.enabled(),
        "permission": grab.permission().as_str(),
    })
}

/// Reads the current selection once (used by the wheel's "grab" affordance).
#[tauri::command]
pub fn smart_grab_now_cmd(state: tauri::State<'_, AppState>) -> serde_json::Value {
    let grab = state.smart_grab_now();
    let mut source = crate::shell::grab::platform_selection_source();
    match grab.grab(source.as_mut()) {
        Ok(paths) => serde_json::json!({
            "paths": paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()
        }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    }
}

/// Emitted to the wheel window when the tray's smart-grab item changes.
pub fn publish_smart_grab(app: &AppHandle, outcome: &serde_json::Value) {
    let _ = app.emit("smart_grab://state", outcome);
}
