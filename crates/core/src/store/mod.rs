//! Persistence: application directories, secret files, and (from milestone 2)
//! settings, transfer history and device trust in SQLite.

mod paths;
mod secure_file;

pub use paths::{AppPaths, StoreError};
pub(crate) use secure_file::write_private;
