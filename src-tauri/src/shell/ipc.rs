//! The Tauri command surface for the main window (`U2`).
//!
//! `ARCHITECTURE.md` §2.2: `shell/` is the only Tauri-aware layer. It translates
//! commands into `model`/`job` calls and nothing else — no conversion logic and
//! no filesystem rules live here.
//!
//! Every command returns a render model rather than a delta, so the webview
//! cannot drift out of sync with the Rust state machine (§10.3: Rust is
//! authoritative).

use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use tauri::State;

use crate::job::queue::{AdmissionReport, Queue, QueueSnapshot};
use crate::model::settings::{self, SettingsStore};
use crate::model::settings_schema::{self, Control};

/// Shared window state. One settings store and one queue per app session.
pub struct AppState {
    pub settings: Mutex<SettingsStore>,
    pub queue: Mutex<Queue>,
    pub started: Instant,
}

impl AppState {
    pub fn new() -> Self {
        let store = SettingsStore::in_config_dir(&settings::config_dir());
        Self {
            settings: Mutex::new(store),
            queue: Mutex::new(Queue::new()),
            started: Instant::now(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn settings_lock<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, SettingsStore> {
    state.settings.lock().unwrap_or_else(|e| e.into_inner())
}

fn queue_lock<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, Queue> {
    state.queue.lock().unwrap_or_else(|e| e.into_inner())
}

/// Everything the settings panel needs for one render.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Value,
    pub defaults: Value,
    pub controls: Vec<Control>,
    pub path: String,
    /// Repairs performed on load/apply, newest last. Shown in diagnostics.
    pub repairs: Vec<String>,
}

fn view(store: &SettingsStore, repairs: Vec<String>) -> SettingsView {
    let settings_value = serde_json::to_value(store.get()).unwrap_or(Value::Null);
    SettingsView {
        defaults: settings_schema::defaults(&crate::model::settings::Settings::default()),
        settings: settings_value,
        controls: settings_schema::controls(),
        path: store.path().to_string_lossy().to_string(),
        repairs,
    }
}

#[tauri::command]
pub fn settings_view(state: State<'_, AppState>) -> SettingsView {
    let store = settings_lock(&state);
    view(&store, store.initial_repairs().to_vec())
}

/// Applies one control change (dotted key + JSON value) and persists it.
#[tauri::command]
pub fn settings_apply(
    key: String,
    value: Value,
    state: State<'_, AppState>,
) -> Result<SettingsView, String> {
    let mut store = settings_lock(&state);
    match store.apply(&key, value) {
        Ok(applied) => Ok(view(&store, applied.repairs)),
        Err(err) => Err(err.to_string()),
    }
}

/// Applies several edits as one atomic write (the batch bar toggles two rules).
#[tauri::command]
pub fn settings_apply_many(
    edits: Vec<(String, Value)>,
    state: State<'_, AppState>,
) -> Result<SettingsView, String> {
    let mut store = settings_lock(&state);
    match store.apply_many(&edits) {
        Ok(applied) => Ok(view(&store, applied.repairs)),
        Err(err) => Err(err.to_string()),
    }
}

/// Re-reads the file from disk (used by the "revert" control and after an
/// external edit).
#[tauri::command]
pub fn settings_reload(state: State<'_, AppState>) -> SettingsView {
    let mut store = settings_lock(&state);
    let repairs = store.reload();
    view(&store, repairs)
}

/// Admission result: what was refused plus the new queue render model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionView {
    pub report: AdmissionReport,
    pub queue: QueueSnapshot,
}

/// Admits dropped paths through the same path the wheel uses (`§11`).
#[tauri::command]
pub fn queue_add_paths(paths: Vec<String>, state: State<'_, AppState>) -> AdmissionView {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    let report = queue.add_paths(
        &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        store.get(),
    );
    let snapshot = queue.snapshot(store.get());
    AdmissionView {
        report,
        queue: snapshot,
    }
}

#[tauri::command]
pub fn queue_snapshot(state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let queue = queue_lock(&state);
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_set_target_all(target: String, state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.set_target_all(&target);
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_set_target(
    id: u64,
    target: Option<String>,
    state: State<'_, AppState>,
) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.set_target(id, target.as_deref());
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_set_strip(id: u64, strip: bool, state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.set_strip(id, strip);
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_set_strip_all(strip: bool, state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.set_strip_all(strip);
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_remove(id: u64, state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.remove(id);
    queue.snapshot(store.get())
}

#[tauri::command]
pub fn queue_clear(state: State<'_, AppState>) -> QueueSnapshot {
    let store = settings_lock(&state);
    let mut queue = queue_lock(&state);
    queue.clear();
    queue.snapshot(store.get())
}

/// Undo the last conversion (`SPEC.md` §5: a 10-second Undo that deletes the
/// outputs it created).
///
/// The decision — *which* files may be deleted — is made by the journal
/// (`job::journal`, `ARCHITECTURE.md` §13), never here: this command only
/// translates the journal's verdict into the toast's answer. A refusal is a
/// normal outcome (window closed, file changed, file is an input), so it comes
/// back as `performed: false` with the reason, not as an error the UI would
/// show as a crash.
#[tauri::command]
pub fn undo_last(state: State<'_, AppState>) -> UndoOutcome {
    let journals = {
        let store = settings_lock(&state);
        crate::job::journal::JournalStore::from_settings(store.get())
    };
    undo_with(&journals, crate::job::journal::now_unix_ms())
}

/// The command's whole decision, without the Tauri `State` wrapper, so it can be
/// tested on a runner with no window (the journal's own tests cover the deletes).
fn undo_with(journals: &crate::job::journal::JournalStore, now_unix_ms: u64) -> UndoOutcome {
    match crate::job::outputs::undo_latest(journals, now_unix_ms) {
        Ok(report) => UndoOutcome {
            performed: true,
            undone: report.deleted.len(),
            message: if report.deleted.is_empty() {
                "Nothing to remove.".into()
            } else {
                format!("Removed {} converted file(s).", report.deleted.len())
            },
        },
        Err(e) => UndoOutcome {
            performed: false,
            undone: 0,
            message: e.to_string(),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoOutcome {
    pub performed: bool,
    pub undone: usize,
    pub message: String,
}

/// Build info shown in the diagnostics pane. Deliberately no network calls.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub settings_path: String,
    pub config_dir: String,
    pub target_formats: Vec<&'static str>,
    pub uptime_seconds: u64,
}

/// Diagnostics string for the window: which engines this build expects to run.
///
/// Moved here from the old `engine.rs` placeholder so that `engine/` stays
/// Tauri-free and can be tested headless (`C2`).
#[tauri::command]
pub fn engine_version() -> String {
    crate::engine::banner()
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let store = settings_lock(&state);
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        settings_path: store.path().to_string_lossy().to_string(),
        config_dir: settings::config_dir().to_string_lossy().to_string(),
        target_formats: crate::model::types::TARGET_FORMATS.to_vec(),
        uptime_seconds: state.started.elapsed().as_secs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_view_serialises_the_control_schema() {
        let dir = std::env::temp_dir().join(format!("ezc-view-{}", std::process::id()));
        let store = SettingsStore::in_config_dir(&dir);
        let view = view(&store, vec![]);
        assert!(!view.controls.is_empty());
        assert!(view.controls.iter().any(|c| c.key == "sound.enabled"));
        let json = serde_json::to_value(&view).unwrap();
        assert!(json["settings"]["sound"]["enabled"].is_boolean());
        assert!(json["controls"][0]["key"].is_string());
        assert!(json["path"].as_str().unwrap().ends_with("settings.json"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_without_a_journal_says_so_and_deletes_nothing() {
        // `undo_last` used to answer "not implemented"; C4 replaced that with the
        // journal. With an empty journal directory the honest answer is "nothing
        // to undo", and it must be a refusal rather than a fake success.
        let dir = std::env::temp_dir().join(format!("ezc-undo-{}", std::process::id()));
        let journals = crate::job::journal::JournalStore::new(dir.join("journal"), 10);
        let outcome = undo_with(&journals, crate::job::journal::now_unix_ms());
        assert!(!outcome.performed);
        assert_eq!(outcome.undone, 0);
        assert!(
            outcome.message.contains("nothing to undo"),
            "message should explain there is nothing to undo, got {:?}",
            outcome.message
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
