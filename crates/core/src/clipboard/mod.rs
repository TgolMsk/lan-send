//! Clipboard abstraction and synchronisation engine.
//!
//! Platform backends (`platform/macos.rs`, `platform/windows.rs`,
//! `platform/ios.rs`) implement one trait; the sync engine (loop prevention,
//! size limits, pairing) is platform independent. Implemented in milestone 3.
