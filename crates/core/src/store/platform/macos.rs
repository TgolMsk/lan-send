//! macOS-specific path lookups.
//!
//! Verified on macOS 26 inside an ad-hoc signed, sandboxed app bundle:
//!
//! | lookup                            | sandboxed result            |
//! |-----------------------------------|-----------------------------|
//! | `$HOME`                           | container `Data`            |
//! | `NSHomeDirectory()`               | container `Data`            |
//! | `NSHomeDirectoryForUser(user)`    | container `Data`            |
//! | `FileManager .downloadsDirectory` | container `Data/Downloads`  |
//! | `getpwuid(getuid())->pw_dir`      | the real home               |

#![allow(unsafe_code)]

use std::ffi::{CStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// The user's real home directory, even inside the App Sandbox.
///
/// The sandbox redirects `$HOME` (and every Foundation home lookup) into
/// `~/Library/Containers/<bundle id>/Data`. Inside that container the sandbox
/// creates `Downloads` as a symlink to the real `~/Downloads` (in every
/// container; `com.apple.security.files.downloads.read-write` only decides
/// whether writing through it is allowed), so writes through the container
/// path did reach the real folder — but the
/// path the app computed, stored and showed in Settings was the container
/// one, and App Review's static scan cannot see Rust `std::fs` writes at all.
/// The passwd database is not redirected, so resolving the home here makes
/// the receive folder explicitly `<real home>/Downloads`: what the user sees
/// is the real path, and the entitlement's use is unambiguous.
pub fn real_home_dir() -> Option<PathBuf> {
    let mut buf = vec![0 as libc::c_char; 4096];
    // SAFETY: `passwd` and `buf` are owned here and outlive the read of
    // `pw_dir`, which only happens once the call reports success and a
    // non-null result. `getpwuid_r` writes nothing beyond `buf.len()`.
    let path = unsafe {
        let mut passwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let code = libc::getpwuid_r(
            libc::getuid(),
            &mut passwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut result,
        );
        if code != 0 || result.is_null() || passwd.pw_dir.is_null() {
            return None;
        }
        PathBuf::from(OsString::from_vec(
            CStr::from_ptr(passwd.pw_dir).to_bytes().to_vec(),
        ))
    };
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_home_is_absolute_and_never_a_container() {
        let home = real_home_dir().expect("macOS always has a passwd entry");
        assert!(home.is_absolute(), "{home:?}");
        assert!(
            !home.to_string_lossy().contains("/Library/Containers/"),
            "{home:?} looks like a sandbox container"
        );
        assert!(home.join("Downloads").parent() == Some(home.as_path()));
    }
}
