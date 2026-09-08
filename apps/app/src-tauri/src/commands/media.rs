//! Thumbnails and metadata for files in the history (ADR-0015).

use crate::AppState;
use crate::error::CmdResult;
use lan_send_core::media::MediaInfo;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn cmd_media_info(state: State<'_, AppState>, path: PathBuf) -> CmdResult<MediaInfo> {
    Ok(state.runtime().await?.media_info(&path).await)
}

#[tauri::command]
pub async fn cmd_media_cache_size(state: State<'_, AppState>) -> CmdResult<u64> {
    Ok(state.runtime().await?.media_cache_size().await)
}

#[tauri::command]
pub async fn cmd_media_cache_clear(state: State<'_, AppState>) -> CmdResult<u64> {
    Ok(state.runtime().await?.media_cache_clear().await?)
}
