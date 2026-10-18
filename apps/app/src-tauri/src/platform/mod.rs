//! Platform integration. Business logic never contains `cfg`; this module
//! and its children are the only place for it (ADR-0001).

#[cfg(desktop)]
pub mod desktop;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_tray_drop;
#[cfg(target_os = "windows")]
mod windows;

use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Called once from Tauri's `setup`, before the runtime starts.
pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    #[cfg(desktop)]
    desktop::setup(app)?;
    #[cfg(target_os = "macos")]
    macos::setup(app)?;
    #[cfg(target_os = "macos")]
    if let Err(err) = macos_tray_drop::setup(app) {
        tracing::warn!("menu bar drop target unavailable: {err:#}");
    }
    #[cfg(target_os = "windows")]
    windows::setup(app)?;
    #[cfg(target_os = "ios")]
    ios::setup(app)?;
    Ok(())
}

/// Called after the runtime started (or failed to), with settings loaded.
pub async fn after_runtime_start(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    macos::restore_receive_dir();
    #[cfg(target_os = "macos")]
    {
        // Without a runtime nothing can be sent, but the panel still
        // explains why; keep the default (on) then.
        let enabled = match app.state::<crate::AppState>().runtime().await {
            Ok(runtime) => runtime.settings().app.menu_bar_drop,
            Err(_) => true,
        };
        macos_tray_drop::apply_settings(app, enabled);
    }
    #[cfg(desktop)]
    desktop::apply_shortcut_from_settings(app).await;
    #[cfg(desktop)]
    desktop::apply_language_from_settings(app).await;
    #[cfg(mobile)]
    let _ = app;
}

/// Shows a system notification. Only macOS sends them for now (results of
/// transfers started from the menu bar, ADR-0022).
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_notification::NotificationExt;
        if let Err(err) = app.notification().builder().title(title).body(body).show() {
            tracing::warn!("notification not shown: {err}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, title, body);
}

/// The user chose a receive folder (`None` = back to the default).
pub fn on_receive_dir_chosen(dir: Option<&std::path::Path>) {
    #[cfg(target_os = "macos")]
    match dir {
        Some(dir) => macos::remember_receive_dir(dir),
        None => macos::forget_receive_dir(),
    }
    #[cfg(not(target_os = "macos"))]
    let _ = dir;
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    #[cfg(desktop)]
    desktop::on_window_event(window, event);
    #[cfg(target_os = "ios")]
    ios::on_window_event(window, event);
    #[cfg(mobile)]
    {
        let _ = (window, event);
    }
}
