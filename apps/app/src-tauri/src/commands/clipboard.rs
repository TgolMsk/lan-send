use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::runtime::ClipboardView;
use tauri::State;

#[tauri::command]
pub async fn cmd_clipboard_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> CmdResult<Vec<ClipboardView>> {
    Ok(state
        .runtime()
        .await?
        .clipboard_history(limit.unwrap_or(50))?)
}

/// Puts a history entry back onto the clipboard.
#[tauri::command]
pub async fn cmd_clipboard_copy(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.runtime().await?.clipboard_copy(&id)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_clipboard_delete(state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    Ok(state.runtime().await?.clipboard_delete(&id)?)
}

#[tauri::command]
pub async fn cmd_clipboard_clear(state: State<'_, AppState>) -> CmdResult<usize> {
    Ok(state.runtime().await?.clipboard_clear()?)
}

/// Pushes the current clipboard to one paired device or all of them.
#[tauri::command]
pub async fn cmd_clipboard_push(
    state: State<'_, AppState>,
    device: Option<String>,
) -> CmdResult<ClipboardView> {
    Ok(state
        .runtime()
        .await?
        .clipboard_push(device.as_deref())
        .await?)
}

#[tauri::command]
pub async fn cmd_clipboard_sync_get(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(state.runtime().await?.clipboard_sync_active())
}

#[tauri::command]
pub async fn cmd_clipboard_sync_set(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    state.runtime().await?.set_clipboard_sync(enabled)?;
    Ok(())
}
