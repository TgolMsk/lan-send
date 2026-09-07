//! iOS specifics: no tray, no global shortcut, no clipboard sync (by
//! decision, ADR-0011). Background networking is limited to the
//! foreground for now.

use tauri::AppHandle;

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}
