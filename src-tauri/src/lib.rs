mod core;
mod db;
mod error;
mod commands;
mod notifier;
mod monitor;
mod ai;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem, CheckMenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

pub type ModeItems = Arc<Mutex<Vec<CheckMenuItem<tauri::Wry>>>>;
use crate::db::init_db;
use crate::ai::sidecar::{SharedSidecar, SidecarState};

#[tauri::command]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

#[tauri::command]
fn open_quick_task(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("quick-task") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}


fn update_mode_checks(app: &tauri::AppHandle, mode: &commands::settings::WorkMode) {
    if let Some(items) = app.try_state::<ModeItems>() {
        let items = items.lock().unwrap();
        let mode_str = format!("mode_{}", mode.as_str());
        for item in items.iter() {
            let _ = item.set_checked(item.id().as_ref() == mode_str.as_str());
        }
    }
}

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
                .plugin(tauri_plugin_shell::init())
                .plugin(tauri_plugin_dialog::init())
                .invoke_handler(
                    tauri::generate_handler![
                        commands::tasks::create_task,
                        commands::tasks::get_tasks,
                        commands::tasks::delete_task,
                        commands::tasks::update_task,
                        commands::tasks::complete_task,
                        commands::tasks::search_tasks,
                        open_quick_task,
                        is_wayland,
                        commands::monitor::record_input,
                        commands::monitor::get_session_stats,
                        commands::monitor::get_activity_state,
                        commands::monitor::get_activity_by_day,
                        commands::monitor::get_task_completions_by_day,
                        commands::ai::ai_rewrite,
                        commands::ai::ai_subtasks,
                        commands::ai::ai_classify,
                        commands::notes::get_notes,
                        commands::notes::create_note,
                        commands::notes::update_note,
                        commands::notes::delete_note,
                        commands::settings::get_settings,
                        commands::settings::save_settings,
                        commands::backup::export,
                        commands::backup::import,
                        commands::model::default_model_url,
                        commands::model::model_status,
                        commands::model::download_model,
                        commands::subtasks::get_subtasks,
                        commands::subtasks::add_subtask,
                        commands::subtasks::toggle_subtask,
                        commands::subtasks::delete_subtask
                    ]
                )
                .setup(|app| {
                    // Трей
                    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
                    // checked=false на старте; правильная галочка выставляется после загрузки настроек
                    let mode_light = CheckMenuItem::with_id(app, "mode_Light", "Light", true, false, None::<&str>)?;
                    let mode_focus = CheckMenuItem::with_id(app, "mode_Focus", "Focus", true, false, None::<&str>)?;
                    let mode_study = CheckMenuItem::with_id(app, "mode_Study", "Study", true, false, None::<&str>)?;
                    let mode_menu = Submenu::with_items(app, "Режим", true, &[&mode_light, &mode_focus, &mode_study])?;
                    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
                    let menu = Menu::with_items(app, &[&open, &mode_menu, &quit])?;

                    let mode_items: ModeItems = Arc::new(Mutex::new(vec![mode_light, mode_focus, mode_study]));
                    app.manage(mode_items);

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
                            id if id.starts_with("mode_") => {
                                let mode = commands::settings::WorkMode::from_str(&id["mode_".len()..]);
                                *app.state::<Arc<Mutex<commands::settings::WorkMode>>>().lock().unwrap() = mode.clone();
                                // Обновить галочки в трее
                                update_mode_checks(app, &mode);
                                let pool = app.state::<sqlx::SqlitePool>().inner().clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = commands::settings::persist_work_mode(&pool, &mode).await;
                                });
                            }
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

                    // Скрывать окно быстрой задачи вместо закрытия (чтобы хоткеи работали)
                    if let Some(quick_win) = app.get_webview_window("quick-task") {
                        let win = quick_win.clone();
                        quick_win.on_window_event(move |event| {
                            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                                api.prevent_close();
                                let _ = win.hide();
                            }
                        });
                    }

                    // Парсинг аргументов CLI для поддержки системных хоткеев на Wayland
                    let args: Vec<String> = std::env::args().collect();
                    if args.iter().any(|arg| arg == "--quick-task" || arg == "-q") {
                        if let Some(main_win) = app.get_webview_window("main") {
                            let _ = main_win.hide();
                        }
                        if let Some(quick_win) = app.get_webview_window("quick-task") {
                            let _ = quick_win.show();
                            let _ = quick_win.set_focus();
                        }
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
            commands::backup::apply_pending_import(&db_path);
            let db_url = format!("sqlite:{}?mode=rwc", db_path.join("data.db").display());
            let pool: sqlx::SqlitePool = init_db(&db_url).await.expect("Failed to init DB");

            app.manage(pool.clone());
            app.manage(Mutex::new(SidecarState::new()) as SharedSidecar);

            let tracker = Arc::new(monitor::activity::ActivityTracker::new());
            app.manage(tracker.clone());
            let settings = commands::settings::load_settings_raw(&pool)
                .await
                .unwrap_or_default();
            // Выставить правильную галочку режима в трее
            update_mode_checks(&app.app_handle(), &settings.work_mode);
            // Режим работы — живое разделяемое состояние: save_settings обновляет
            // его сразу, без перезапуска приложения.
            let work_mode = Arc::new(Mutex::new(settings.work_mode.clone()));
            app.manage(work_mode.clone());
            monitor::activity::start_activity_loop(
                app.app_handle().clone(),
                tracker,
                pool.clone(),
                settings.idle_threshold_secs,
                settings.log_interval_secs,
                work_mode.clone(),
            );

            notifier::scheduler::start_scheduler(app.app_handle().clone(), pool.clone(), work_mode.clone());
            notifier::nudge::start_nudger(app.app_handle().clone(), pool.clone(), work_mode.clone());
            notifier::pomodoro::start_pomodoro(app.app_handle().clone(), work_mode, pool);
            app.run(|_, _| {});
        });
}