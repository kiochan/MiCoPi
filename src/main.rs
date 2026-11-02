#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(all(windows, feature = "desktop"))]
    {
        if let Err(error) = micopi::desktop::run() {
            micopi::platform::windows::desktop_bridge::report_error(&error.to_string());
        }
    }
    #[cfg(not(windows))]
    eprintln!("MiCoPi currently requires Windows 10/11 x64.");
}
