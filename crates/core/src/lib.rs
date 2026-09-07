//! # lan-send core
//!
//! Pure Rust core of lan-send. It implements the LocalSend v2.2 protocol
//! (discovery, upload API, TLS identity) and the private extensions on top of
//! it (clipboard sync, media metadata, resumable uploads).
//!
//! Hard rules (see `CLAUDE.md`):
//! - No dependency on Tauri, a window system or any GUI framework.
//! - Everything must be usable from the CLI alone.
//! - Cross-platform differences go behind traits; platform implementations
//!   live in `platform` submodules gated with `#[cfg(target_os = ...)]`.
//!   Business logic never contains `cfg`.
//! - `unwrap()` is forbidden outside tests (enforced by clippy).
//!
//! Module map:
//!
//! | module        | responsibility                                     | milestone |
//! |---------------|----------------------------------------------------|-----------|
//! | [`protocol`]  | v2.2 DTOs, constants, private extension fields     | 1         |
//! | [`discovery`] | UDP multicast + HTTP register / subnet scan        | 1         |
//! | [`transport`] | TLS identity, HTTPS server and client               | 1–2       |
//! | [`store`]     | app directories; settings, history, trust (SQLite) | 1–2       |
//! | [`transfer`]  | outgoing file collection, incoming placement       | 2         |
//! | [`clipboard`] | clipboard model, backends, sync engine, wire      | 3         |
//! | [`media`]     | MIME sniffing, thumbnails, audio metadata          | 4         |

pub mod clipboard;
pub mod discovery;
pub mod media;
pub mod protocol;
pub mod store;
pub mod transfer;
pub mod transport;
