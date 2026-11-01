//! Tray icon (Windows) / menu-bar item (macOS): the app's presence when no window is
//! open (`SPEC.md` §2 — "always-on-top wheel" plus a place to change settings).
//!
//! Tauri 2 gives one API for both: a `TrayIcon` with a menu. The menu is the product
//! surface, so it carries the three things `SPEC.md` §2/§5 promise reachable without
//! the main window: summon the wheel, the smart-grab toggle (with its permission
//! state), and Quit. No update item, no account item, no network (`ARCHITECTURE.md`
//! §11).
//!
//! The toggle is a **check item backed by the settings store**, not by the tray's own
//! memory: the same `smart_grab.enabled` value drives the settings panel and the menu,
//! so the two cannot disagree.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

/// Menu item ids. Also the strings the event handler matches on.
pub const ITEM_SUMMON: &str = "summon";
pub const ITEM_SMART_GRAB: &str = "smart_grab";
pub const ITEM_SETTINGS: &str = "settings";
pub const ITEM_QUIT: &str = "quit";

/// Handles kept alive for the lifetime of the app. Dropping a `CheckMenuItem` does not
/// remove it from the menu, but dropping the *tray* does remove the icon, so the tray
/// is stored here too.
pub struct TrayState {
    pub smart_grab: CheckMenuItem<Wry>,
}

/// Builds the tray icon and its menu. Called once from `.setup()`.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let summon = MenuItem::with_id(
        app,
        ITEM_SUMMON,
        "Show conversion wheel",
        true,
        Some("Ctrl+Alt+Shift+D"),
    )?;
    let settings = MenuItem::with_id(app, ITEM_SETTINGS, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ITEM_QUIT, "Quit EZ Convert", true, None::<&str>)?;

    // `SPEC.md` §2: smart grab is opt-in. The menu must show the real value at build
    // time, so it reads the store rather than assuming the default.
    let smart_grab_enabled = app
        .try_state::<crate::shell::ipc::AppState>()
        .map(|state| state.smart_grab_now().enabled())
        .unwrap_or(false);
    let smart_grab = CheckMenuItem::with_id(
        app,
        ITEM_SMART_GRAB,
        "Smart grab the current selection",
        true,
        smart_grab_enabled,
        None::<&str>,
    )?;

    let menu = Menu::with_items(
        app,
        &[
            &summon,
            &PredefinedMenuItem::separator(app)?,
            &smart_grab,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("ezc-tray")
        .menu(&menu)
        // macOS convention is left-click-opens-menu; Windows convention is
        // left-click-activates. `show_menu_on_left_click(true)` is the behaviour that
        // works on both without an extra platform branch.
        .show_menu_on_left_click(true)
        .tooltip("EZ Convert")
        .on_menu_event(|app, event| match event.id().as_ref() {
            ITEM_SUMMON => {
                crate::shell::summon::summon(app);
            }
            ITEM_SMART_GRAB => {
                let outcome = crate::shell::summon::toggle_smart_grab(app);
                crate::shell::summon::publish_smart_grab(app, &outcome);
            }
            ITEM_SETTINGS => crate::shell::summon::show_main_window(app),
            ITEM_QUIT => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    app.manage(TrayState { smart_grab });
    Ok(())
}

/// Keeps the menu's check mark in step after a change made in the settings panel.
pub fn sync_smart_grab(app: &AppHandle, enabled: bool) {
    if let Some(state) = app.try_state::<TrayState>() {
        // A failed `set_checked` only means the menu item is gone (app shutting down).
        let _ = state.smart_grab.set_checked(enabled);
    }
}
