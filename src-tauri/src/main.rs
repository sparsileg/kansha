// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `kansha add`, `kansha run`: write to the book's inbox and exit,
    // without a window (CLI-030).
    if let Some(code) = kansha_lib::cli_main() {
        std::process::exit(code);
    }
    kansha_lib::run();
}
