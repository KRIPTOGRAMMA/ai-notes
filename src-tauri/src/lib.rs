mod core;
mod db;
mod commands;
mod notifier;

use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
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
                .plugin(tauri_plugin_notification::init())
                .plugin(tauri_plugin_autostart::init(
                    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                    None,
                ))
                .plugin(tauri_plugin_global_shortcut::Builder::new().build())
                .invoke_handler(
                    tauri::generate_handler![
                        commands::tasks::create_task,
                        commands::tasks::get_tasks,
                        commands::tasks::delete_task,
                        commands::tasks::update_task,
                        commands::tasks::complete_task,
                        commands::tasks::search_tasks
                    ]
                )
                .setup(|app| {
                    // Трей
                    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
                    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
                    let menu = Menu::with_items(app, &[&open, &quit])?;

                    TrayIconBuilder::new()
                        .icon(app.default_window_icon().unwrap().clone())
                        .menu(&menu)
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "open" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            "quit" => app.exit(0),
                            _ => {}
                        })
                        .on_tray_icon_event(|tray, event| {
                            // Клик по иконке — открыть окно
                            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                                let app = tray.app_handle();
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        })
                        .build(app)?;

                    // Скрывать главное окно вместо закрытия (чтобы трей работал)
                    if let Some(main_win) = app.get_webview_window("main") {
                        let win = main_win.clone();
                        main_win.on_window_event(move |event| {
                            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                                api.prevent_close();
                                let _ = win.hide();
                            }
                        });
                    }

                    // Глобальные хоткеи
                    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
                    let new_task_shortcut = "Ctrl+Shift+N".parse::<Shortcut>().unwrap();

                    app.global_shortcut().on_shortcuts(
                        [new_task_shortcut],
                        move |app, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                if let Some(w) = app.get_webview_window("quick-task") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                    )?;

                    Ok(())
                })
                .build(tauri::generate_context!())
                .expect("error while building tauri application");

            let db_path = app.path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&db_path)
                .expect("Failed to create app data dir");
            let db_url = format!("sqlite:{}?mode=rwc", db_path.join("data.db").display());
            let pool: sqlx::SqlitePool = init_db(&db_url).await.expect("Failed to init DB");

            app.manage(pool.clone());
            notifier::scheduler::start_scheduler(app.app_handle().clone(), pool);
            app.run(|_, _| {});
        });
}