//! Windows specifics. Nothing yet beyond what `desktop.rs` does.

use tauri::AppHandle;

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}
