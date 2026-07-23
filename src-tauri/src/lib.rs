mod core;
mod db;
mod error;
mod commands;
mod notifier;
mod monitor;
mod ai;
mod status;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem, CheckMenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};

// Обычные (не checkbox) пункты меню — some tray-хосты (напр. waybar) неверно
// рендерят checkbox-пункты (GtkCheckMenuItem), показывая текст статуса
// вместо названия ("ВЫКЛ" вместо "Light"/"Focus"/"Study"). Обходим: активный
// режим показываем префиксом "✓ " прямо в тексте пункта.
pub type ModeItems = Arc<Mutex<Vec<MenuItem<tauri::Wry>>>>;
pub type QuietItems = Arc<Mutex<Vec<CheckMenuItem<tauri::Wry>>>>;
// Начальный режим окна быстрого ввода: "task" | "note". Живёт как managed-state,
// чтобы QuickCapture мог прочитать его при монтировании (гонка emit-до-mount).
pub type QuickMode = Arc<Mutex<String>>;
use crate::db::init_db;
use crate::ai::sidecar::{SharedSidecar, SidecarState};

#[tauri::command]
fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

// Режим трекинга активности: extended — системный idle/resume через
// ext-idle-notify-v1 (Wayland), basic — только ввод в окне приложения.
pub struct ExtendedTracking(pub bool);

#[tauri::command]
fn get_tracking_mode(mode: tauri::State<'_, ExtendedTracking>) -> &'static str {
    if mode.0 { "extended" } else { "basic" }
}

// Провайдер активного окна (имя), если capability detection нашёл рабочий.
pub struct WindowTracking(pub Option<&'static str>);

#[tauri::command]
fn get_window_tracking(state: tauri::State<'_, WindowTracking>) -> Option<&'static str> {
    state.0
}

fn normalize_quick_mode(mode: &str) -> &'static str {
    if mode == "note" { "note" } else { "task" }
}

// Единый путь открытия окна быстрого ввода: фиксируем режим, оповещаем окно
// (если уже смонтировано) и показываем его. Используется и командой из фронта,
// и глобальными хоткеями.
fn show_quick_capture(app: &tauri::AppHandle, mode: &str) {
    let mode = normalize_quick_mode(mode);
    if let Some(state) = app.try_state::<QuickMode>() {
        *state.lock().unwrap() = mode.to_string();
    }
    let _ = app.emit_to("quick-task", "quick-mode", mode);
    if let Some(w) = app.get_webview_window("quick-task") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn open_quick_capture(app: tauri::AppHandle, mode: String) {
    show_quick_capture(&app, &mode);
}

// Режим быстрого ввода из аргументов CLI (--quick-note / --quick-task / -q).
// Общий парсер для первого запуска и для аргументов, пересланных вторым
// экземпляром через single-instance (биндами WM на Wayland).
fn quick_mode_from_args(args: &[String]) -> Option<&'static str> {
    if args.iter().any(|a| a == "--quick-note") {
        Some("note")
    } else if args.iter().any(|a| a == "--quick-task" || a == "-q") {
        Some("task")
    } else {
        None
    }
}

#[tauri::command]
fn get_quick_mode(mode: tauri::State<'_, QuickMode>) -> String {
    mode.lock().unwrap().clone()
}


fn update_mode_checks(app: &tauri::AppHandle, mode: &commands::settings::WorkMode) {
    if let Some(items) = app.try_state::<ModeItems>() {
        let items = items.lock().unwrap();
        let active_id = format!("mode_{}", mode.as_str());
        for item in items.iter() {
            let id = item.id().as_ref().to_string();
            let name = id.strip_prefix("mode_").unwrap_or(&id);
            let label = if id == active_id { format!("✓ {name}") } else { name.to_string() };
            let _ = item.set_text(label);
        }
    }
}

// Галочка на активном пункте паузы уведомлений. active_id — id пункта меню
// ("quiet_off" | "quiet_30" | ... | "quiet_inf"); чужой id снимает все галочки.
fn update_quiet_checks(app: &tauri::AppHandle, active_id: &str) {
    if let Some(items) = app.try_state::<QuietItems>() {
        let items = items.lock().unwrap();
        for item in items.iter() {
            let _ = item.set_checked(item.id().as_ref() == active_id);
        }
    }
}

// Какой пункт меню паузы должен быть отмечен для данного quiet_until.
// preset — сохранённый id пресета (quiet_preset в settings): по нему таймерная
// пауза восстанавливает галочку после перезапуска. Легаси-значение без пресета
// при активной таймерной паузе — "quiet_timed" (ни один пункт не отмечен).
fn quiet_check_id(quiet_until: &str, preset: &str, now: chrono::DateTime<chrono::Utc>) -> &'static str {
    if quiet_until == commands::settings::QUIET_FOREVER {
        return "quiet_inf";
    }
    match chrono::DateTime::parse_from_rfc3339(quiet_until) {
        Ok(t) if now < t.with_timezone(&chrono::Utc) => match preset {
            "quiet_30" => "quiet_30",
            "quiet_60" => "quiet_60",
            "quiet_120" => "quiet_120",
            _ => "quiet_timed",
        },
        _ => "quiet_off",
    }
}

// Остаток активной таймерной паузы в минутах (округление вверх);
// None — пауза не активна или бессрочная.
fn quiet_remaining_mins(quiet_until: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    if quiet_until == commands::settings::QUIET_FOREVER {
        return None;
    }
    let t = chrono::DateTime::parse_from_rfc3339(quiet_until)
        .ok()?
        .with_timezone(&chrono::Utc);
    let secs = (t - now).num_seconds();
    if secs <= 0 {
        return None;
    }
    Some((secs + 59) / 60)
}

fn quiet_base_label(id: &str) -> &'static str {
    match id {
        "quiet_30" => "30 минут",
        "quiet_60" => "1 час",
        "quiet_120" => "2 часа",
        "quiet_inf" => "Бессрочно",
        _ => "Выкл",
    }
}

// Подписи пунктов паузы: активный таймерный пресет показывает остаток
// («1 час — осталось 42 мин»), остальные — базовую подпись.
fn update_quiet_labels(app: &tauri::AppHandle, active_id: &str, remaining_mins: Option<i64>) {
    if let Some(items) = app.try_state::<QuietItems>() {
        let items = items.lock().unwrap();
        for item in items.iter() {
            let id = item.id().as_ref();
            let base = quiet_base_label(id);
            let text = match remaining_mins {
                Some(m) if id == active_id => format!("{base} — осталось {m} мин"),
                _ => base.to_string(),
            };
            let _ = item.set_text(text);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `ai-notes --status` — короткоживущий CLI для статус-баров (waybar):
    // печатает JSON и выходит, не поднимая Tauri. Проверяется до всего
    // остального, чтобы не задеть single-instance работающего приложения.
    if std::env::args().any(|a| a == "--status") {
        status::print_status();
        return;
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let app = tauri::Builder::default()
                // Регистрируется первым: второй запуск (напр. `ai-notes --quick-task`
                // из бинда Hyprland) не стартует новый экземпляр, а пересылает
                // аргументы сюда — открываем быстрый ввод или главное окно.
                .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
                    if let Some(mode) = quick_mode_from_args(&argv) {
                        show_quick_capture(app, mode);
                    } else if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }))
                .plugin(tauri_plugin_opener::init())
                .plugin(tauri_plugin_notification::init())
                .plugin(tauri_plugin_autostart::init(
                    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                    None,
                ))
                .plugin(tauri_plugin_global_shortcut::Builder::new().build())
                .plugin(tauri_plugin_shell::init())
                .plugin(tauri_plugin_dialog::init())
                .plugin(tauri_plugin_clipboard_manager::init())
                .invoke_handler(
                    tauri::generate_handler![
                        commands::tasks::create_task,
                        commands::tasks::get_tasks,
                        commands::tasks::delete_task,
                        commands::tasks::get_deleted_tasks,
                        commands::tasks::restore_task,
                        commands::tasks::purge_deleted_task,
                        commands::tasks::update_task,
                        commands::tasks::complete_task,
                        commands::tasks::search_tasks,
                        commands::tasks::search_tasks_snippet,
                        commands::tasks::reorder_tasks,
                        commands::projects::get_projects,
                        commands::projects::create_project,
                        commands::projects::update_project,
                        commands::projects::delete_project,
                        commands::projects::get_goal_history,
                        commands::categories::get_categories,
                        commands::categories::create_category,
                        commands::categories::update_category,
                        commands::categories::delete_category,
                        commands::pomodoro::get_pomodoro_state,
                        commands::pomodoro::pomodoro_toggle_pause,
                        commands::pomodoro::pomodoro_skip,
                        commands::pomodoro::pomodoro_start,
                        commands::pomodoro::pomodoro_stop,
                        commands::pomodoro::get_pomodoro_stats,
                        open_quick_capture,
                        get_quick_mode,
                        is_wayland,
                        get_tracking_mode,
                        get_window_tracking,
                        commands::monitor::record_input,
                        commands::monitor::get_session_stats,
                        commands::monitor::get_activity_state,
                        commands::monitor::get_activity_by_day,
                        commands::monitor::get_task_completions_by_day,
                        commands::monitor::get_app_usage,
                        commands::monitor::get_completions_for_day,
                        commands::monitor::get_hourly_activity,
                        commands::monitor::get_app_category_time,
                        commands::monitor::get_category_distribution,
                        commands::monitor::get_active_idle_ratio,
                        commands::ai::ai_rewrite,
                        commands::ai::ai_subtasks,
                        commands::ai::ai_classify,
                        commands::ai::dashboard_insight,
                        commands::ai::summarize_day,
                        commands::ai::summarize_week,
                        commands::ai::ai_edit_selection,
                        commands::ai::ai_summarize_note,
                        commands::ai::ai_extract_tasks,
                        commands::planner::ai_plan_day,
                        commands::planner::ai_what_now,
                        commands::notes::get_notes,
                        commands::notes::create_note,
                        commands::notes::update_note,
                        commands::notes::delete_note,
                        commands::notes::search_notes,
                        commands::notes::search_notes_snippet,
                        commands::notes::rename_note_links,
                        commands::notes::get_note_revisions,
                        commands::notes::get_note_revision_content,
                        commands::notes::restore_note_revision,
                        commands::notes::export_notes_md,
                        commands::notes::import_notes_md,
                        commands::notes::export_note_html,
                        commands::note_links::ai_suggest_links,
                        commands::settings::get_settings,
                        commands::settings::save_settings,
                        commands::backup::export,
                        commands::backup::import,
                        commands::backup::do_auto_backup,
                        commands::model::list_model_options,
                        commands::model::model_status,
                        commands::model::download_model,
                        commands::subtasks::get_subtasks,
                        commands::subtasks::add_subtask,
                        commands::subtasks::toggle_subtask,
                        commands::subtasks::delete_subtask,
                        commands::subtasks::rename_subtask,
                        commands::routines::get_routines,
                        commands::routines::create_routine,
                        commands::routines::update_routine,
                        commands::routines::delete_routine,
                        commands::tracking::start_task_tracking,
                        commands::tracking::stop_task_tracking,
                        commands::tracking::get_active_session,
                        commands::tracking::get_task_seconds,
                        commands::tracking::get_project_seconds,
                        commands::images::save_note_image,
                        commands::images::get_images_dir,
                        commands::checklists::get_checklist_templates,
                        commands::checklists::create_checklist_template,
                        commands::checklists::delete_checklist_template,
                        commands::smart_lists::get_smart_lists,
                        commands::smart_lists::create_smart_list,
                        commands::smart_lists::delete_smart_list
                    ]
                )
                .setup(|app| {
                    // Трей
                    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
                    // Обычные пункты (не checkbox) — см. комментарий у ModeItems: активный
                    // режим помечается префиксом "✓ " в тексте, выставляется update_mode_checks
                    // после загрузки настроек.
                    let mode_light = MenuItem::with_id(app, "mode_Light", "Light", true, None::<&str>)?;
                    let mode_focus = MenuItem::with_id(app, "mode_Focus", "Focus", true, None::<&str>)?;
                    let mode_study = MenuItem::with_id(app, "mode_Study", "Study", true, None::<&str>)?;
                    let mode_menu = Submenu::with_items(app, "Режим", true, &[&mode_light, &mode_focus, &mode_study])?;
                    // Пауза уведомлений: галочка выставляется после загрузки настроек
                    let quiet_30 = CheckMenuItem::with_id(app, "quiet_30", quiet_base_label("quiet_30"), true, false, None::<&str>)?;
                    let quiet_60 = CheckMenuItem::with_id(app, "quiet_60", quiet_base_label("quiet_60"), true, false, None::<&str>)?;
                    let quiet_120 = CheckMenuItem::with_id(app, "quiet_120", quiet_base_label("quiet_120"), true, false, None::<&str>)?;
                    let quiet_inf = CheckMenuItem::with_id(app, "quiet_inf", quiet_base_label("quiet_inf"), true, false, None::<&str>)?;
                    let quiet_off = CheckMenuItem::with_id(app, "quiet_off", quiet_base_label("quiet_off"), true, false, None::<&str>)?;
                    let quiet_menu = Submenu::with_items(
                        app,
                        "Пауза уведомлений",
                        true,
                        &[&quiet_30, &quiet_60, &quiet_120, &quiet_inf, &quiet_off],
                    )?;
                    let pomo_pause = MenuItem::with_id(app, "pomo_pause", "Помодоро: пауза/продолжить", true, None::<&str>)?;
                    let pomo_skip = MenuItem::with_id(app, "pomo_skip", "Помодоро: пропустить фазу", true, None::<&str>)?;
                    let pomo_menu = Submenu::with_items(app, "Помодоро", true, &[&pomo_pause, &pomo_skip])?;
                    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
                    let menu = Menu::with_items(app, &[&open, &mode_menu, &quiet_menu, &pomo_menu, &quit])?;

                    let mode_items: ModeItems = Arc::new(Mutex::new(vec![mode_light, mode_focus, mode_study]));
                    app.manage(mode_items);
                    let quiet_items: QuietItems =
                        Arc::new(Mutex::new(vec![quiet_30, quiet_60, quiet_120, quiet_inf, quiet_off]));
                    app.manage(quiet_items);

                    // Начальный режим окна быстрого ввода (по умолчанию — задача)
                    let quick_mode: QuickMode = Arc::new(Mutex::new("task".to_string()));
                    app.manage(quick_mode);

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
                            id if id.starts_with("quiet_") => {
                                use commands::settings::QUIET_FOREVER;
                                let value = match id {
                                    "quiet_off" => String::new(),
                                    "quiet_inf" => QUIET_FOREVER.to_string(),
                                    "quiet_30" => (chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339(),
                                    "quiet_60" => (chrono::Utc::now() + chrono::Duration::minutes(60)).to_rfc3339(),
                                    "quiet_120" => (chrono::Utc::now() + chrono::Duration::minutes(120)).to_rfc3339(),
                                    _ => return,
                                };
                                update_quiet_checks(app, id);
                                update_quiet_labels(app, id, quiet_remaining_mins(&value, chrono::Utc::now()));
                                let pool = app.state::<sqlx::SqlitePool>().inner().clone();
                                let preset = id.to_string();
                                tauri::async_runtime::spawn(async move {
                                    let _ = commands::settings::persist_quiet_until(&pool, &value).await;
                                    let _ = commands::settings::persist_quiet_preset(&pool, &preset).await;
                                });
                            }
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
                            // PomodoroCmdTx управляется позже в run() (после сборки app),
                            // но к моменту клика по трею приложение уже запущено.
                            "pomo_pause" => {
                                if let Some(tx) = app.try_state::<commands::pomodoro::PomodoroCmdTx>() {
                                    let _ = tx.0.send(notifier::pomodoro::PomodoroCmd::TogglePause);
                                }
                            }
                            "pomo_skip" => {
                                if let Some(tx) = app.try_state::<commands::pomodoro::PomodoroCmdTx>() {
                                    let _ = tx.0.send(notifier::pomodoro::PomodoroCmd::Skip);
                                }
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
                    if let Some(mode) = quick_mode_from_args(&args) {
                        if let Some(main_win) = app.get_webview_window("main") {
                            let _ = main_win.hide();
                        }
                        show_quick_capture(&app.app_handle(), mode);
                    }

                    // Глобальные хоткеи: Ctrl+Shift+N — задача, Ctrl+Shift+M — заметка
                    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
                    let task_shortcut = "Ctrl+Shift+N".parse::<Shortcut>().unwrap();
                    let note_shortcut = "Ctrl+Shift+M".parse::<Shortcut>().unwrap();
                    let note_id = note_shortcut.id();

                    app.global_shortcut().on_shortcuts(
                        [task_shortcut, note_shortcut],
                        move |app, shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                let mode = if shortcut.id() == note_id { "note" } else { "task" };
                                show_quick_capture(app, mode);
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

            // Расширенный трекинг: системный idle/resume от компоситора.
            // Не поддерживается (X11, старый компоситор) — базовый режим.
            let extended = is_wayland() && monitor::wayland_idle::start(tracker.clone());
            app.manage(ExtendedTracking(extended));
            eprintln!("[monitor] режим трекинга: {}", if extended { "расширенный (ext-idle-notify)" } else { "базовый (окно в фокусе)" });

            // Трекинг по приложениям: capability detection, нет провайдера — колонка app пустая.
            let window_provider = monitor::window::detect_provider();
            app.manage(WindowTracking(window_provider.as_ref().map(|p| p.name())));
            eprintln!(
                "[monitor] трекинг приложений: {}",
                window_provider.as_ref().map(|p| p.name()).unwrap_or("недоступен")
            );
            let settings = commands::settings::load_settings_raw(&pool)
                .await
                .unwrap_or_default();
            // Выставить правильные галочки режима и паузы в трее.
            // Пресет паузы хранится отдельно (quiet_preset) — таймерная пауза
            // после перезапуска восстанавливает и галочку, и подпись с остатком.
            update_mode_checks(&app.app_handle(), &settings.work_mode);
            let quiet_preset = commands::settings::get_setting(&pool, "quiet_preset")
                .await
                .unwrap_or_default();
            let now = chrono::Utc::now();
            let quiet_id = quiet_check_id(&settings.quiet_until, &quiet_preset, now);
            update_quiet_checks(&app.app_handle(), quiet_id);
            update_quiet_labels(
                &app.app_handle(),
                quiet_id,
                quiet_remaining_mins(&settings.quiet_until, now),
            );

            // Вотчер паузы: когда quiet_until проходит, снимаем галочку с пресета
            // и отмечаем «Выкл»; пока таймерная пауза активна — раз в минуту
            // обновляем остаток в подписи пункта.
            {
                let app_handle = app.app_handle().clone();
                let pool_watch = pool.clone();
                let mut last_id = quiet_id;
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        let value = commands::settings::get_setting(&pool_watch, "quiet_until")
                            .await
                            .unwrap_or_default();
                        let preset = commands::settings::get_setting(&pool_watch, "quiet_preset")
                            .await
                            .unwrap_or_default();
                        let now = chrono::Utc::now();
                        let id = quiet_check_id(&value, &preset, now);
                        if id != last_id && id != "quiet_timed" {
                            update_quiet_checks(&app_handle, id);
                        }
                        last_id = id;
                        update_quiet_labels(&app_handle, id, quiet_remaining_mins(&value, now));
                    }
                });
            }
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
                window_provider,
            );

            notifier::scheduler::start_scheduler(app.app_handle().clone(), pool.clone(), work_mode.clone());
            notifier::nudge::start_nudger(app.app_handle().clone(), pool.clone(), work_mode.clone());
            notifier::triggers::start_triggers(app.app_handle().clone(), pool.clone(), work_mode.clone());
            let pomodoro_tx = notifier::pomodoro::start_pomodoro(app.app_handle().clone(), work_mode, pool.clone());
            app.manage(commands::pomodoro::PomodoroCmdTx(pomodoro_tx));

            // Авто-бэкап: раз в 60с проверяем, не пора ли сделать копию
            {
                let app_handle = app.app_handle().clone();
                let pool_bk = pool.clone();
                tokio::spawn(async move {
                    loop {
                        if commands::backup::auto_backup_due(&pool_bk).await {
                            let data_dir = match app_handle.path().app_data_dir() {
                                Ok(d) => d,
                                Err(_) => { tokio::time::sleep(Duration::from_secs(60)).await; continue; }
                            };
                            let _ = commands::backup::auto_backup_impl(&pool_bk, &data_dir).await;
                        }
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                });
            }
            app.run(|_, _| {});
        });
}
#[cfg(test)]
mod tests {
    use super::{quick_mode_from_args, quiet_check_id, quiet_remaining_mins};
    use chrono::{TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 14, 12, 0, 0).unwrap()
    }

    #[test]
    fn quick_mode_parsing() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(quick_mode_from_args(&args(&["ai-notes", "--quick-task"])), Some("task"));
        assert_eq!(quick_mode_from_args(&args(&["ai-notes", "-q"])), Some("task"));
        assert_eq!(quick_mode_from_args(&args(&["ai-notes", "--quick-note"])), Some("note"));
        // заметка приоритетнее, как и в старом коде запуска
        assert_eq!(quick_mode_from_args(&args(&["ai-notes", "--quick-task", "--quick-note"])), Some("note"));
        assert_eq!(quick_mode_from_args(&args(&["ai-notes"])), None);
    }

    #[test]
    fn check_id_forever_and_off() {
        assert_eq!(quiet_check_id(crate::commands::settings::QUIET_FOREVER, "", now()), "quiet_inf");
        assert_eq!(quiet_check_id("", "", now()), "quiet_off");
        assert_eq!(quiet_check_id("мусор", "quiet_30", now()), "quiet_off");
        // истёкшая пауза — «Выкл», даже если пресет сохранён
        assert_eq!(quiet_check_id("2026-07-14T11:00:00+00:00", "quiet_60", now()), "quiet_off");
    }

    #[test]
    fn check_id_restores_preset_for_active_timed_pause() {
        let until = "2026-07-14T12:30:00+00:00";
        assert_eq!(quiet_check_id(until, "quiet_30", now()), "quiet_30");
        assert_eq!(quiet_check_id(until, "quiet_60", now()), "quiet_60");
        assert_eq!(quiet_check_id(until, "quiet_120", now()), "quiet_120");
        // легаси-значение без пресета (или с мусором) — галочки нет
        assert_eq!(quiet_check_id(until, "", now()), "quiet_timed");
        assert_eq!(quiet_check_id(until, "quiet_off", now()), "quiet_timed");
    }

    #[test]
    fn remaining_mins_none_when_inactive() {
        assert_eq!(quiet_remaining_mins("", now()), None);
        assert_eq!(quiet_remaining_mins(crate::commands::settings::QUIET_FOREVER, now()), None);
        assert_eq!(quiet_remaining_mins("2026-07-14T11:59:00+00:00", now()), None);
        assert_eq!(quiet_remaining_mins("2026-07-14T12:00:00+00:00", now()), None);
    }

    #[test]
    fn remaining_mins_rounds_up() {
        // ровно 30 минут — сразу после клика по пресету
        assert_eq!(quiet_remaining_mins("2026-07-14T12:30:00+00:00", now()), Some(30));
        // 90 секунд — округляем вверх до 2 минут
        assert_eq!(quiet_remaining_mins("2026-07-14T12:01:30+00:00", now()), Some(2));
        // последняя минута
        assert_eq!(quiet_remaining_mins("2026-07-14T12:00:30+00:00", now()), Some(1));
    }
}
