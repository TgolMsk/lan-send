//! macOS specifics. Nothing yet beyond what `desktop.rs` does; dock and
//! activation-policy handling will land with the frontend milestone.

use tauri::AppHandle;

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}
