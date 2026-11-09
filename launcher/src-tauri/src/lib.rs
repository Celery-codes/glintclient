mod commands;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses the running window instead
        // of starting another launcher that fights over the same files.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(AppState::new(data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::system::get_settings,
            commands::system::save_settings,
            commands::system::open_data_folder,
            commands::meta::list_game_versions,
            commands::meta::list_loader_versions,
            commands::instances::list_presets,
            commands::instances::list_instances,
            commands::instances::create_instance,
            commands::instances::update_instance,
            commands::instances::delete_instance,
            commands::instances::duplicate_instance,
            commands::instances::open_instance_folder,
            commands::launch::launch_instance,
            commands::launch::stop_instance,
            commands::launch::instance_states,
            commands::launch::instance_log,
            commands::auth::auth_begin,
            commands::auth::auth_cancel,
            commands::auth::list_accounts,
            commands::auth::set_active_account,
            commands::auth::remove_account,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Glint");
}
