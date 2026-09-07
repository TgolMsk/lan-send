use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::store::TransferRecord;
use tauri::State;

#[tauri::command]
pub async fn cmd_history_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CmdResult<Vec<TransferRecord>> {
    Ok(state.runtime().await?.history(limit.unwrap_or(200))?)
}

#[tauri::command]
pub async fn cmd_history_delete(state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    Ok(state.runtime().await?.delete_history(&id)?)
}

#[tauri::command]
pub async fn cmd_history_clear(state: State<'_, AppState>) -> CmdResult<usize> {
    Ok(state.runtime().await?.clear_history()?)
}
