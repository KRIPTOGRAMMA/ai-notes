// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod core;
mod db;
mod commands;

use tauri::Manager;
use crate::db::init_db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let app = tauri::Builder::default()
                .plugin(tauri_plugin_opener::init())
                .invoke_handler(
                    tauri::generate_handler![
                        commands::tasks::create_task,
                        commands::tasks::get_tasks
                    ]
                )
                .build(tauri::generate_context!())
                .expect("error while building tauri application");

            let db_path = app.path()
                .app_data_dir()
                .expect("Failed to get app data dir");

            std::fs::create_dir_all(&db_path)
                .expect("Failed to create app data dir");

            let db_url = format!("sqlite:{}?mode=rwc", db_path.join("data.db").display());
            let pool = init_db(&db_url).await.expect("Failed to init DB");
            
            app.manage(pool);
            app.run(|_, _| {});
        });
}
