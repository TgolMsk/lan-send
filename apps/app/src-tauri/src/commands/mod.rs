//! IPC commands, one module per core area, named `cmd_<module>_<action>`.

pub mod app;
pub mod clipboard;
pub mod devices;
pub mod history;
pub mod media;
pub mod pair;
pub mod transfer;
