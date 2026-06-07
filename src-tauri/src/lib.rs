// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod core;
mod db;
mod commands;

use crate::commands::tasks::create_task;
use crate::db::init_db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let pool = init_db("sqlite:data.db").await.expect("Failed to init DB");

            tauri::Builder::default()
                .plugin(tauri_plugin_opener::init())
                .manage(pool)
                .invoke_handler(tauri::generate_handler![create_task])
                .run(tauri::generate_context!())
                .expect("error while running tauri application");
        });
}
