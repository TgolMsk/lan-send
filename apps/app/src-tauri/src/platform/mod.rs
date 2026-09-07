//! Platform integration. Business logic never contains `cfg`; this module
//! and its children are the only place for it (ADR-0001).

#[cfg(desktop)]
pub mod desktop;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use tauri::AppHandle;

/// Called once from Tauri's `setup`, before the runtime starts.
pub fn setup(app: &AppHandle) -> anyhow::Result<()> {
    #[cfg(desktop)]
    desktop::setup(app)?;
    #[cfg(target_os = "macos")]
    macos::setup(app)?;
    #[cfg(target_os = "windows")]
    windows::setup(app)?;
    #[cfg(target_os = "ios")]
    ios::setup(app)?;
    Ok(())
}

/// Called after the runtime started (or failed to), with settings loaded.
pub async fn after_runtime_start(app: &AppHandle) {
    #[cfg(desktop)]
    desktop::apply_shortcut_from_settings(app).await;
    #[cfg(mobile)]
    let _ = app;
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    #[cfg(desktop)]
    desktop::on_window_event(window, event);
    #[cfg(mobile)]
    {
        let _ = (window, event);
    }
}
