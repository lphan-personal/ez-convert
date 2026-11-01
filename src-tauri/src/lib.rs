//! EZ Convert core library.
//!
//! P0 skeleton only. The wheel UI, the conversion engine and platform
//! integration land later: U1 (wheel + motion), P2 (bundling/signing),
//! S1/S2/S3 (per-OS conversion backends).
//!
//! Everything below is deliberately side-effect free so `cargo test` runs on
//! a headless CI runner with no window, no webview session and no ffmpeg.

pub mod engine;

/// Builds and runs the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine::engine_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
