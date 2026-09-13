//! Identity, settings, runtime control and host integration (dialogs,
//! opening files).

use crate::AppState;
use crate::error::{AppError, CmdResult};
use crate::state::RuntimeStateView;
use lan_send_core::runtime::{ErrorCode, IdentityView};
use lan_send_core::store::Settings;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
#[cfg(mobile)]
use tauri_plugin_dialog::PickerMode;
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub mobile: bool,
    pub version: &'static str,
}

#[tauri::command]
pub fn cmd_app_platform() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        mobile: cfg!(any(target_os = "ios", target_os = "android")),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
pub async fn cmd_app_runtime_state(state: State<'_, AppState>) -> CmdResult<RuntimeStateView> {
    Ok(state.state_view().await)
}

#[tauri::command]
pub async fn cmd_app_restart(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state.start_runtime(&app).await?;
    crate::platform::after_runtime_start(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn cmd_app_identity(state: State<'_, AppState>) -> CmdResult<IdentityView> {
    Ok(state.runtime().await?.identity())
}

#[tauri::command]
pub async fn cmd_app_settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.runtime().await?.settings())
}

/// Saves the settings; restarts the runtime when the network side changed.
/// Returns whether it restarted.
#[tauri::command]
pub async fn cmd_app_settings_update(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<bool> {
    let runtime = state.runtime().await?;
    if settings.receive_dir.is_none() {
        crate::platform::on_receive_dir_chosen(None);
    }
    let restart = runtime.update_settings(settings.clone())?;
    state
        .close_to_tray
        .store(settings.app.close_to_tray, Ordering::Relaxed);
    if restart {
        state.start_runtime(&app).await?;
    }
    crate::platform::after_runtime_start(&app).await;
    Ok(restart)
}

/// Opens the system file picker; `folders` picks directories instead
/// What the user is picking. `Media` opens the iOS photo library; the others
/// open the file browser. Folders are desktop only.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickKind {
    #[default]
    Files,
    Folders,
    Media,
}

/// Picks files, folders or photos to send. Photos come from the system photo
/// library (iOS `PHPicker`), which copies each chosen item into the app's
/// temporary directory and hands back a normal path, so the send path is the
/// same for all three.
#[tauri::command]
pub async fn cmd_app_pick_files(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: PickKind,
) -> CmdResult<Vec<PathBuf>> {
    let strings = state.locale().await.strings();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file().set_title(match kind {
        PickKind::Files => strings.dialog_files,
        PickKind::Folders => strings.dialog_folders,
        PickKind::Media => strings.dialog_media,
    });
    match kind {
        PickKind::Folders => {
            #[cfg(desktop)]
            dialog.pick_folders(move |paths| {
                let _ = tx.send(paths);
            });
            #[cfg(mobile)]
            {
                drop((dialog, tx));
                return Err(AppError::coded(
                    ErrorCode::Unsupported,
                    "folder picking is not available on this platform",
                ));
            }
        }
        PickKind::Media => {
            #[cfg(mobile)]
            dialog
                .set_picker_mode(PickerMode::Media)
                .pick_files(move |paths| {
                    let _ = tx.send(paths);
                });
            #[cfg(desktop)]
            {
                drop((dialog, tx));
                return Err(AppError::coded(
                    ErrorCode::Unsupported,
                    "the photo library is only available on mobile",
                ));
            }
        }
        PickKind::Files => dialog.pick_files(move |paths| {
            let _ = tx.send(paths);
        }),
    }
    let picked = rx.await.map_err(|_| "the dialog was closed")?;
    let mut paths = Vec::new();
    for path in picked.unwrap_or_default() {
        paths.push(
            path.into_path()
                .map_err(|err| AppError::message(err.to_string()))?,
        );
    }
    Ok(paths)
}

/// Picks one directory for the receive folder setting (desktop only).
#[tauri::command]
pub async fn cmd_app_pick_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<PathBuf>> {
    #[cfg(mobile)]
    {
        let _ = (app, state);
        Err(AppError::coded(
            ErrorCode::Unsupported,
            "folder picking is not available on this platform",
        ))
    }
    #[cfg(desktop)]
    {
        let title = state.locale().await.strings().dialog_receive_dir;
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_title(title)
            .pick_folder(move |path| {
                let _ = tx.send(path);
            });
        let picked = rx.await.map_err(|_| "the dialog was closed")?;
        let path = match picked {
            Some(path) => Some(
                path.into_path()
                    .map_err(|err| AppError::message(err.to_string()))?,
            ),
            None => None,
        };
        if let Some(path) = &path {
            crate::platform::on_receive_dir_chosen(Some(path));
        }
        Ok(path)
    }
}

/// Opens a file or folder with its default application.
#[tauri::command]
pub fn cmd_app_open_path(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)?;
    Ok(())
}

/// Shows a file in the file manager (Finder / Explorer).
#[tauri::command]
pub fn cmd_app_reveal_path(app: AppHandle, path: PathBuf) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(path)?;
    Ok(())
}
