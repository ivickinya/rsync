mod filesystem;
mod git_deploy;
mod github_auth;
mod profiles;
mod transfers;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Err(error) = window.state::<transfers::WatchStore>().stop_active() {
                    log::error!("Could not stop Watch & Sync while closing the app: {error}");
                }
            }
        })
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let profiles = profiles::ProfileStore::load(profiles::storage_path(&app_data_dir))
                .map_err(std::io::Error::other)?;
            app.manage(profiles);
            app.manage(transfers::TransferStore::default());
            app.manage(transfers::WatchStore::default());

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            filesystem::list_local_directory,
            git_deploy::deploy_git_repository,
            profiles::get_profiles,
            profiles::save_profile,
            profiles::delete_profile,
            transfers::open_ssh_terminal,
            github_auth::set_github_token,
            github_auth::clear_github_token,
            transfers::test_ssh_connection,
            transfers::list_remote_directory,
            transfers::compare_transfer_file,
            transfers::preview_rsync,
            transfers::execute_rsync,
            transfers::start_directory_watcher,
            transfers::stop_directory_watcher
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}

pub fn run_git_askpass() -> Option<i32> {
    github_auth::run_askpass()
}
