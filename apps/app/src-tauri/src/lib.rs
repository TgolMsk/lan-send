//! Tauri shell of lan-send (ADR-0013). It holds the core runtime, forwards
//! its events to the webview as `event:<name>` and exposes the runtime's
//! API as `cmd_<module>_<action>` commands. Platform specifics live in
//! `platform/`.

mod commands;
mod error;
mod platform;
mod state;

pub use state::AppState;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    let result = builder
        .manage(AppState::default())
        .setup(|app| {
            platform::setup(app.handle())?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                if let Err(err) = state.start_runtime(&handle).await {
                    tracing::error!("runtime did not start: {err:#}");
                }
                platform::after_runtime_start(&handle).await;
            });
            Ok(())
        })
        .on_window_event(platform::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::app::cmd_app_identity,
            commands::app::cmd_app_platform,
            commands::app::cmd_app_runtime_state,
            commands::app::cmd_app_restart,
            commands::app::cmd_app_settings_get,
            commands::app::cmd_app_settings_update,
            commands::app::cmd_app_pick_files,
            commands::app::cmd_app_pick_folder,
            commands::app::cmd_app_open_path,
            commands::app::cmd_app_reveal_path,
            commands::devices::cmd_devices_list,
            commands::devices::cmd_devices_refresh,
            commands::devices::cmd_devices_set_favorite,
            commands::devices::cmd_devices_set_alias,
            commands::devices::cmd_devices_forget,
            commands::transfer::cmd_transfer_send,
            commands::transfer::cmd_transfer_list,
            commands::transfer::cmd_transfer_cancel,
            commands::transfer::cmd_transfer_provide_pin,
            commands::transfer::cmd_transfer_dismiss,
            commands::transfer::cmd_transfer_respond_incoming,
            commands::transfer::cmd_transfer_respond_conflict,
            commands::pair::cmd_pair_start,
            commands::pair::cmd_pair_confirm,
            commands::pair::cmd_pair_respond,
            commands::pair::cmd_pair_unpair,
            commands::history::cmd_history_list,
            commands::history::cmd_history_delete,
            commands::history::cmd_history_clear,
            commands::media::cmd_media_info,
            commands::media::cmd_media_cache_size,
            commands::media::cmd_media_cache_clear,
            commands::clipboard::cmd_clipboard_history,
            commands::clipboard::cmd_clipboard_copy,
            commands::clipboard::cmd_clipboard_delete,
            commands::clipboard::cmd_clipboard_clear,
            commands::clipboard::cmd_clipboard_push,
            commands::clipboard::cmd_clipboard_sync_get,
            commands::clipboard::cmd_clipboard_sync_set,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("lan-send could not start: {err}");
        std::process::exit(1);
    }
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,lan_send_core=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
