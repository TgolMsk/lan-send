//! Persistence (ADR-0006): application directories, `settings.json`, the
//! SQLite database (transfer history, known devices, partial uploads) and
//! owner-only secret files.

mod db;
mod paths;
mod platform;
mod secure_file;
mod settings;

pub use db::{
    ClipboardRecord, Database, Direction, KnownDevice, PartialUpload, TransferRecord,
    TransferStatus, unix_now,
};
pub use paths::{AppPaths, StoreError};
pub(crate) use secure_file::write_private;
pub use settings::{
    AppSettings, ClipboardSettings, ConflictPolicy, OrganizeRules, Settings, Theme,
};
