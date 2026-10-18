//! iOS specifics: no tray, no global shortcut, no clipboard sync (by
//! decision, ADR-0011). Background networking is limited to the
//! foreground for now. Recreate listeners when returning from suspension.

use crate::AppState;
use tauri::{AppHandle, Manager, Window, WindowEvent};

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::Resumed) {
        return;
    }
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        tracing::info!("iOS returned to the foreground; restoring network listeners");
        let state = app.state::<AppState>();
        if let Err(err) = state.start_runtime(&app).await {
            tracing::error!("could not restore the iOS network runtime: {err:#}");
        }
    });
}
