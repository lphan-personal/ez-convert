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

pub mod diag;
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
use shell::summon::{
    hotkey_rebind, hotkey_view, smart_grab_now_cmd, smart_grab_set, smart_grab_view, wheel_placement,
    wheel_summon,
};

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .setup(|app| {
            // `ARCHITECTURE.md` §6.2 operational contract: the install-cold
            // *first* document conversion costs 5,111 ms on Windows because
            // LibreOffice bootstraps its profile. That cost belongs to app
            // start, not to the user's first job — so it is paid here, on a
            // worker thread, once per profile. Best effort by construction:
            // `warm_up_at_startup` never fails the app and never blocks the
            // window; a failure is a diagnostics line, not an error dialog.
            use tauri::Manager;
            let root = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
            let cache = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            // macOS keeps every engine in the separate engine pack
            // (docs/MACOS-PACKAGING.md); `adopt_pack` fills in whatever the
            // bundle itself did not provide and never overrides a bundled engine.
            let pack = app.path().app_data_dir().ok();
            std::thread::spawn(move || {
                let mut paths = engine::engines_in(&root);
                if let Some(pack) = &pack {
                    paths.adopt_pack(pack);
                }
                let outcome = engine::soffice::warm_up_at_startup(&paths, &cache);
                eprintln!("document warm-up: {outcome}");
            });

            // Intake surface (`SPEC.md` §2, card U3): the tray/menu-bar item and the
            // global hotkey. Neither is fatal to launch — a taken hotkey or a tray the
            // shell refuses must not stop the app from opening, or the user cannot
            // reach the settings that would fix it.
            let handle = app.handle().clone();
            if let Err(error) = shell::tray::build(&handle) {
                eprintln!("tray unavailable: {error}");
            }
            shell::summon::start_hotkeys(&handle);
            Ok(())
        })
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
            // intake: tray, hotkey, smart grab (U3)
            wheel_summon,
            wheel_placement,
            hotkey_view,
            hotkey_rebind,
            smart_grab_set,
            smart_grab_view,
            smart_grab_now_cmd,
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
