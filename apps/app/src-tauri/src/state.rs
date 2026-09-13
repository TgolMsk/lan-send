//! Application state shared by commands: the core runtime and the event
//! pump that turns [`RuntimeEvent`]s into Tauri events.

use crate::error::CmdResult;
use anyhow::Context;
use lan_send_core::protocol::DeviceType;
use lan_send_core::runtime::{Runtime, RuntimeConfig, RuntimeEvent};
use lan_send_core::store::AppPaths;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::sync::{RwLock, mpsc};

/// Event name prefix required by the brief (`event:transfer-progress`, …).
pub const EVENT_PREFIX: &str = "event:";

/// Emitted with `{ running, message }` whenever the runtime starts, stops
/// or fails to start.
pub const RUNTIME_STATE_EVENT: &str = "event:runtime-state";

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStateView {
    pub running: bool,
    pub message: Option<String>,
}

pub struct AppState {
    runtime: RwLock<Option<Runtime>>,
    last_error: RwLock<Option<String>>,
    /// Mirror of `settings.app.close_to_tray` for the synchronous window
    /// event handler.
    pub close_to_tray: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: RwLock::new(None),
            last_error: RwLock::new(None),
            close_to_tray: AtomicBool::new(true),
        }
    }
}

impl AppState {
    /// The running runtime, or an error the interface can show.
    pub async fn runtime(&self) -> CmdResult<Runtime> {
        match self.runtime.read().await.clone() {
            Some(runtime) => Ok(runtime),
            None => {
                let message = self
                    .last_error
                    .read()
                    .await
                    .clone()
                    .unwrap_or_else(|| "the runtime is not running".to_string());
                Err(crate::error::AppError::coded(
                    lan_send_core::runtime::ErrorCode::RuntimeStopped,
                    message,
                ))
            }
        }
    }

    /// The interface language from the settings, the system's before the
    /// runtime is up.
    pub async fn locale(&self) -> crate::Locale {
        match self.runtime.read().await.as_ref() {
            Some(runtime) => crate::Locale::from_setting(&runtime.settings().app.language),
            None => crate::Locale::system(),
        }
    }

    pub async fn state_view(&self) -> RuntimeStateView {
        RuntimeStateView {
            running: self.runtime.read().await.is_some(),
            message: self.last_error.read().await.clone(),
        }
    }

    /// (Re)starts the runtime and the event pump. A failure is remembered
    /// and reported through `event:runtime-state`.
    pub async fn start_runtime(&self, app: &AppHandle) -> anyhow::Result<()> {
        self.stop_runtime().await;
        let result = self.start_inner(app).await;
        let view = match &result {
            Ok(()) => RuntimeStateView {
                running: true,
                message: None,
            },
            Err(err) => RuntimeStateView {
                running: false,
                message: Some(format!("{err:#}")),
            },
        };
        *self.last_error.write().await = view.message.clone();
        let _ = app.emit(RUNTIME_STATE_EVENT, &view);
        result
    }

    async fn start_inner(&self, app: &AppHandle) -> anyhow::Result<()> {
        let paths = AppPaths::resolve().context("cannot resolve the application directories")?;
        let (tx, mut rx) = mpsc::channel::<RuntimeEvent>(1024);
        let config = RuntimeConfig::new(paths, device_type(), tx);
        let runtime = Runtime::start(config)
            .await
            .context("cannot start the network runtime")?;
        self.close_to_tray
            .store(runtime.settings().app.close_to_tray, Ordering::Relaxed);
        *self.runtime.write().await = Some(runtime);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = rx.recv().await {
                let name = format!("{EVENT_PREFIX}{}", event.name());
                if let Err(err) = app.emit(&name, &event) {
                    tracing::warn!("could not emit {name}: {err}");
                }
            }
        });
        Ok(())
    }

    pub async fn stop_runtime(&self) {
        if let Some(runtime) = self.runtime.write().await.take() {
            runtime.stop().await;
        }
    }
}

fn device_type() -> DeviceType {
    if cfg!(target_os = "ios") {
        DeviceType::Mobile
    } else {
        DeviceType::Desktop
    }
}
