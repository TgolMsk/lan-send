//! macOS specifics: keeping access to the chosen receive folder inside the
//! App Sandbox through a security-scoped bookmark (ADR-0014). Outside the
//! sandbox the same code runs and is harmless.

#![allow(unsafe_code)]

use lan_send_core::store::AppPaths;
use objc2::runtime::Bool;
use objc2_foundation::{
    NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::AppHandle;

const BOOKMARK_FILE: &str = "receive-dir.bookmark";

/// The URL whose security scope is currently open; kept for the process
/// lifetime so writes into the folder keep working.
static ACCESSED: Mutex<Option<objc2::rc::Retained<NSURL>>> = Mutex::new(None);

pub fn setup(_app: &AppHandle) -> anyhow::Result<()> {
    Ok(())
}

fn bookmark_path() -> Option<PathBuf> {
    AppPaths::resolve()
        .ok()
        .map(|paths| paths.config_dir.join(BOOKMARK_FILE))
}

/// Called when the user picked a receive folder: stores a bookmark and
/// opens its security scope right away.
pub fn remember_receive_dir(dir: &Path) {
    let Some(file) = bookmark_path() else {
        return;
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
    let data = url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
        NSURLBookmarkCreationOptions::WithSecurityScope,
        None,
        None,
    );
    match data {
        Ok(data) => {
            let bytes = unsafe { data.as_bytes_unchecked() }.to_vec();
            if let Err(err) = std::fs::write(&file, bytes) {
                tracing::warn!("could not store the receive folder bookmark: {err}");
            }
            open_scope(url);
        }
        Err(err) => tracing::debug!("no security-scoped bookmark for {}: {err}", dir.display()),
    }
}

/// Forgets the bookmark (the user went back to the default folder).
pub fn forget_receive_dir() {
    if let Some(file) = bookmark_path() {
        let _ = std::fs::remove_file(file);
    }
    if let Ok(mut guard) = ACCESSED.lock()
        && let Some(url) = guard.take()
    {
        unsafe { url.stopAccessingSecurityScopedResource() };
    }
}

/// Resolves the stored bookmark at start-up and opens its security scope.
pub fn restore_receive_dir() {
    let Some(file) = bookmark_path() else {
        return;
    };
    let Ok(bytes) = std::fs::read(&file) else {
        return;
    };
    let data = NSData::with_bytes(&bytes);
    let mut stale = Bool::NO;
    let resolved = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::WithSecurityScope,
            None,
            &mut stale,
        )
    };
    match resolved {
        Ok(url) => {
            if stale.as_bool() {
                // Refresh the bookmark while we still can.
                if let Some(path) = url.path() {
                    remember_receive_dir(Path::new(&path.to_string()));
                    return;
                }
            }
            open_scope(url);
        }
        Err(err) => {
            tracing::warn!("receive folder bookmark no longer resolves: {err}");
            let _ = std::fs::remove_file(file);
        }
    }
}

fn open_scope(url: objc2::rc::Retained<NSURL>) {
    if let Ok(mut guard) = ACCESSED.lock() {
        if let Some(previous) = guard.take() {
            unsafe { previous.stopAccessingSecurityScopedResource() };
        }
        if unsafe { url.startAccessingSecurityScopedResource() } {
            *guard = Some(url);
        }
    }
}
