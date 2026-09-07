//! Desktop entry point; the mobile entry point lives in `lib.rs`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lan_send_app_lib::run()
}
