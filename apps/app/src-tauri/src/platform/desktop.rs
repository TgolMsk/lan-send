//! Desktop-only pieces shared by macOS and Windows: tray icon, global
//! shortcut, close-to-tray.

use crate::AppState;
use crate::state::EVENT_PREFIX;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open lan-send", true, None::<&str>)?;
    let push = MenuItem::with_id(
        app,
        "push",
        "Push clipboard to paired devices",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &push,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the bundle has no icon"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("lan-send")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "push" => push_clipboard(app.clone()),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Pushes the clipboard to every paired device; errors go to the
/// interface as `event:error`.
pub fn push_clipboard(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = match state.runtime().await {
            Ok(runtime) => runtime
                .clipboard_push(None)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),
            Err(err) => Err(err.message),
        };
        if let Err(message) = result {
            let _ = app.emit(
                &format!("{EVENT_PREFIX}error"),
                serde_json::json!({ "type": "error", "scope": "clipboard", "message": message }),
            );
        }
    });
}

/// Registers the shortcut from the settings (none when empty or invalid).
pub async fn apply_shortcut_from_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(runtime) = state.runtime().await else {
        return;
    };
    let accelerator = runtime.settings().app.global_shortcut;
    if let Err(err) = apply_shortcut(app, &accelerator) {
        tracing::warn!("global shortcut {accelerator:?} not registered: {err}");
        let _ = app.emit(
            &format!("{EVENT_PREFIX}error"),
            serde_json::json!({ "type": "error", "scope": "shortcut", "message": format!("shortcut {accelerator} not registered: {err}") }),
        );
    }
}

pub fn apply_shortcut(app: &AppHandle, accelerator: &str) -> anyhow::Result<()> {
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all()?;
    let accelerator = accelerator.trim();
    if accelerator.is_empty() {
        return Ok(());
    }
    let shortcut: Shortcut = accelerator.parse()?;
    shortcuts.on_shortcut(shortcut, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            push_clipboard(app.clone());
        }
    })?;
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let close_to_tray = window
            .app_handle()
            .state::<AppState>()
            .close_to_tray
            .load(Ordering::Relaxed);
        if close_to_tray {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}
