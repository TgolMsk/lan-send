//! Which naming rules apply on the platform this binary runs on. The only
//! `cfg` in the file name module.

use super::Rules;

pub(super) const fn current() -> Rules {
    if cfg!(target_os = "windows") {
        Rules::Windows
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        Rules::Hfs
    } else if cfg!(unix) {
        Rules::Posix
    } else {
        Rules::Universal
    }
}
