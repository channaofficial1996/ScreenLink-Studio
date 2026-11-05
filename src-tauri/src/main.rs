// Hide the console window for release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    screenlink_lib::run()
}
