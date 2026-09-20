// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(code) = slbs_soundboard_lib::run_driver_cli_action(&arguments) {
        std::process::exit(code);
    }
    slbs_soundboard_lib::run()
}
