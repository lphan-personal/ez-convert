// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--diagnostics` runs the installed-build self-test and exits without ever
    // creating a window, so a packaged app can prove itself on a headless
    // machine (docs/MACOS-PACKAGING.md, P3 acceptance).
    if ez_convert_lib::diag::is_cli() {
        std::process::exit(ez_convert_lib::diag::run());
    }
    ez_convert_lib::run()
}
