// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(exit_code) = rsync_desktop::run_git_askpass() {
        std::process::exit(exit_code);
    }
    rsync_desktop::run();
}
