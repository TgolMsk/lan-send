use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::runtime::{SendRequest, TransferView};
use tauri::State;

/// Starts a send; returns the transfer id whose progress arrives as events.
#[tauri::command]
pub async fn cmd_transfer_send(
    state: State<'_, AppState>,
    request: SendRequest,
) -> CmdResult<String> {
    Ok(state.runtime().await?.send(request)?)
}

#[tauri::command]
pub async fn cmd_transfer_list(state: State<'_, AppState>) -> CmdResult<Vec<TransferView>> {
    Ok(state.runtime().await?.transfers())
}

#[tauri::command]
pub async fn cmd_transfer_cancel(state: State<'_, AppState>, transfer_id: String) -> CmdResult<()> {
    state.runtime().await?.cancel_transfer(&transfer_id).await?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_transfer_provide_pin(
    state: State<'_, AppState>,
    transfer_id: String,
    pin: Option<String>,
) -> CmdResult<()> {
    state.runtime().await?.provide_pin(&transfer_id, pin)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_transfer_dismiss(
    state: State<'_, AppState>,
    transfer_id: String,
) -> CmdResult<bool> {
    Ok(state.runtime().await?.dismiss_transfer(&transfer_id))
}

#[tauri::command]
pub async fn cmd_transfer_respond_incoming(
    state: State<'_, AppState>,
    session_id: String,
    accept: bool,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .respond_incoming(&session_id, accept)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_transfer_respond_conflict(
    state: State<'_, AppState>,
    session_id: String,
    file_id: String,
    overwrite: bool,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .respond_conflict(&session_id, &file_id, overwrite)?;
    Ok(())
}
