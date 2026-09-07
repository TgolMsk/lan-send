use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::runtime::DeviceView;
use tauri::State;

#[tauri::command]
pub async fn cmd_devices_list(state: State<'_, AppState>) -> CmdResult<Vec<DeviceView>> {
    Ok(state.runtime().await?.devices())
}

#[tauri::command]
pub async fn cmd_devices_refresh(state: State<'_, AppState>) -> CmdResult<()> {
    state.runtime().await?.refresh_devices();
    Ok(())
}

#[tauri::command]
pub async fn cmd_devices_set_favorite(
    state: State<'_, AppState>,
    fingerprint: String,
    favorite: bool,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .set_favorite(&fingerprint, favorite)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_devices_set_alias(
    state: State<'_, AppState>,
    fingerprint: String,
    alias: Option<String>,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .set_custom_alias(&fingerprint, alias)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_devices_forget(state: State<'_, AppState>, fingerprint: String) -> CmdResult<()> {
    state.runtime().await?.forget_device(&fingerprint).await?;
    Ok(())
}
