//! Proof that settings survive a real process restart.
//!
//! The card's acceptance is "settings persist across restart", so this test does
//! not merely re-read a file in the same process: it writes from a **child
//! process** (`child_process_writes_settings`, run with `--ignored --exact`) and
//! then loads the settings in the parent. A fresh process, a fresh in-memory
//! store, the same `settings.json` on disk.

use ez_convert_lib::model::settings::{self, SettingsStore};
use serde_json::json;
use std::path::{Path, PathBuf};

const CHILD_ENV: &str = "EZC_SETTINGS_PROBE_DIR";

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("ezc-restart-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn settings_changed_in_one_process_are_seen_by_the_next() {
    let dir = unique_dir("parent");

    let exe = std::env::current_exe().expect("test binary path");
    let status = std::process::Command::new(&exe)
        .args([
            "--exact",
            "child_process_writes_settings",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD_ENV, &dir)
        .status()
        .expect("spawn child test process");
    assert!(status.success(), "child process failed: {status:?}");

    // The file the child wrote is the only shared state.
    let path = settings::settings_path(&dir);
    assert!(
        path.is_file(),
        "child should have written {}",
        path.display()
    );

    // A brand-new store, as a relaunched app would build.
    let store = SettingsStore::in_config_dir(&dir);
    assert!(
        !store.get().sound.enabled,
        "sound toggle must survive restart"
    );
    assert_eq!(store.get().output.batch_subfolder, "batch-out");
    assert_eq!(store.get().output.batch_threshold, 4);
    assert!(store.get().folder_drop.include_subfolders);
    assert!(!store.get().smart_grab.enabled, "smart grab stays opt-in");
    assert_eq!(
        store.get().wheel.order,
        vec!["pdf", "png", "jpg", "webp", "mp3", "mp4"]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "run as a child process by settings_changed_in_one_process_are_seen_by_the_next"]
fn child_process_writes_settings() {
    let dir = match std::env::var(CHILD_ENV) {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return,
    };
    let mut store = SettingsStore::in_config_dir(&dir);
    store.apply("sound.enabled", json!(false)).unwrap();
    store
        .apply("output.batch_subfolder", json!("batch-out"))
        .unwrap();
    store.apply("output.batch_threshold", json!(4)).unwrap();
    store
        .apply("folder_drop.include_subfolders", json!(true))
        .unwrap();
    store
        .apply(
            "wheel.order",
            json!(["pdf", "png", "jpg", "webp", "mp3", "mp4"]),
        )
        .unwrap();
    assert!(store.path().is_file(), "settings file written");
    assert_eq!(store.path(), Path::new(&settings::settings_path(&dir)));
}
