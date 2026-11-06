// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--mcp") => {
            if let Err(error) = rsync_desktop::run_agent_mcp_stdio() {
                rsync_desktop::exit_agent_mode_with_error(error);
            }
            return;
        }
        Some("--copilot-hook") => {
            if let Err(error) = rsync_desktop::run_copilot_hook() {
                rsync_desktop::exit_agent_mode_with_error(error);
            }
            return;
        }
        _ => {}
    }

    if let Some(exit_code) = rsync_desktop::run_git_askpass() {
        std::process::exit(exit_code);
    }
    rsync_desktop::run();
}
