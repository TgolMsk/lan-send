use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::runtime::PairView;
use tauri::State;

/// Asks a device to pair; both sides show the returned code.
#[tauri::command]
pub async fn cmd_pair_start(state: State<'_, AppState>, device: String) -> CmdResult<PairView> {
    Ok(state.runtime().await?.pair_start(&device).await?)
}

/// After `event:pair-response`: whether the codes matched.
#[tauri::command]
pub async fn cmd_pair_confirm(
    state: State<'_, AppState>,
    fingerprint: String,
    matches: bool,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .pair_confirm(&fingerprint, matches)
        .await?;
    Ok(())
}

/// Answers `event:pair-request`.
#[tauri::command]
pub async fn cmd_pair_respond(
    state: State<'_, AppState>,
    fingerprint: String,
    accept: bool,
) -> CmdResult<()> {
    state
        .runtime()
        .await?
        .respond_pair_request(&fingerprint, accept)?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_pair_unpair(state: State<'_, AppState>, fingerprint: String) -> CmdResult<()> {
    state.runtime().await?.unpair(&fingerprint).await?;
    Ok(())
}
