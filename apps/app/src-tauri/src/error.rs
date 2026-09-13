//! Errors crossing the IPC boundary: a message the interface can show and,
//! when the cause is a known one, a language-neutral code it translates.

use lan_send_core::runtime::{ErrorCode, RuntimeError};

/// Serialised as `{ "code": "unreachable" | null, "message": "..." }`.
#[derive(Debug, serde::Serialize)]
pub struct AppError {
    pub code: Option<ErrorCode>,
    pub message: String,
}

impl AppError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }

    pub fn coded(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
        }
    }
}

impl From<RuntimeError> for AppError {
    fn from(err: RuntimeError) -> Self {
        let code = err.code();
        Self {
            code: (code != ErrorCode::Unknown).then_some(code),
            message: err.to_string(),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        let code = ErrorCode::from_io(&err);
        Self {
            code: (code != ErrorCode::Unknown).then_some(code),
            message: err.to_string(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        // A runtime error wrapped with context keeps its code.
        let code = err
            .downcast_ref::<RuntimeError>()
            .map(RuntimeError::code)
            .filter(|code| *code != ErrorCode::Unknown);
        Self {
            code,
            message: format!("{err:#}"),
        }
    }
}

macro_rules! message_only {
    ($($ty:ty),* $(,)?) => {$(
        impl From<$ty> for AppError {
            fn from(err: $ty) -> Self {
                Self::message(err.to_string())
            }
        }
    )*};
}

message_only!(
    String,
    &str,
    tauri::Error,
    tauri_plugin_opener::Error,
    tauri_plugin_dialog::Error,
);

pub type CmdResult<T> = Result<T, AppError>;
