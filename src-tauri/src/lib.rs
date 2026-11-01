//! EZ Convert core library.
//!
//! Layout follows `ARCHITECTURE.md` §2.2:
//!
//! - `model/` — pure data and rules (settings, naming, target names);
//! - `job/` — queue admission and per-file state;
//! - `shell/` — the only Tauri-aware layer (commands for the window).
//!
//! `model/` and `job/` must stay Tauri-free (§2.1); a test below greps for it so
//! the boundary is enforced mechanically, not by intention.

pub mod engine;
pub mod job;
pub mod model;
pub mod shell;

use shell::ipc::{
    app_info, engine_version, queue_add_paths, queue_clear, queue_remove, queue_set_strip,
    queue_set_strip_all,
    queue_set_target, queue_set_target_all, queue_snapshot, settings_apply, settings_apply_many,
    settings_reload, settings_view, undo_last, AppState,
};

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            engine_version,
            // settings (U2)
            settings_view,
            settings_apply,
            settings_apply_many,
            settings_reload,
            // queue (U2)
            queue_add_paths,
            queue_snapshot,
            queue_set_target_all,
            queue_set_target,
            queue_set_strip,
            queue_set_strip_all,
            queue_remove,
            queue_clear,
            // completion + diagnostics
            undo_last,
            app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// `ARCHITECTURE.md` §2.1: the mechanical boundary check.
    #[test]
    fn model_and_job_never_import_tauri() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for dir in ["model", "job"] {
            let mut stack = vec![root.join(dir)];
            while let Some(path) = stack.pop() {
                let entries = std::fs::read_dir(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
                        let text = std::fs::read_to_string(&p).unwrap();
                        for line in text.lines() {
                            let trimmed = line.trim_start();
                            assert!(
                                !(trimmed.starts_with("use tauri")
                                    || trimmed.starts_with("use tauri_")),
                                "{} imports tauri: {line}",
                                p.display()
                            );
                        }
                    }
                }
            }
        }
    }
}
