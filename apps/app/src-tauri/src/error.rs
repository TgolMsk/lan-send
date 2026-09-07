//! Errors crossing the IPC boundary: a message the interface can show.

/// Serialised as `{ "message": "..." }`.
#[derive(Debug, serde::Serialize)]
pub struct AppError {
    pub message: String,
}

impl<E: std::fmt::Display> From<E> for AppError {
    fn from(err: E) -> Self {
        Self {
            message: err.to_string(),
        }
    }
}

pub type CmdResult<T> = Result<T, AppError>;
