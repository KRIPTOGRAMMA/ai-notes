use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;
use tokio::time::{sleep, Duration};
use crate::commands::settings::{WorkMode, get_u64_setting, get_bool_setting, set_setting};
use crate::notifier::scheduler::send_notification;

// Пользовательская команда управления циклом (пауза/возобновление/пропуск фазы/
// ручной старт-стоп вне Study).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PomodoroCmd {
    TogglePause,
    Skip,
    Start,
    Stop,
}

// Пишем строку в pomodoro_log при каждом завершении work-фазы (переход work→break).
// task_id — активная сессия трекинга задачи, если она в этот момент идёт.
async fn log_completed_work(pool: &SqlitePool) {
    let task_id: Option<String> = sqlx::query_scalar(
        "SELECT task_id FROM task_sessions WHERE ended_at IS NULL LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let _ = sqlx::query(
        "INSERT INTO pomodoro_log (id, finished_at, task_id) VALUES (?, ?, ?)"
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(task_id)
    .execute(pool)
    .await;
}

// Канал команд из Tauri-команд (UI) в цикл. Управляемое состояние —
// тип-обёртка, чтобы app.manage() не конфликтовал с другими Sender<T>.
pub struct PomodoroCmdTx(pub tokio::sync::mpsc::UnboundedSender<PomodoroCmd>);

// Персистентный снимок цикла — читается фронтом (poll) и `ai-notes --status`.
// phase: "work" | "break" | "paused" | "off". until — RFC3339 конца текущей
// фазы (для "paused"/"off" не используется фронтом, но пишем на всякий случай
// последнее актуальное значение).
async fn persist_state(pool: &SqlitePool, phase: &str, until: chrono::DateTime<chrono::Utc>) {
    let _ = set_setting(pool, "pomodoro_phase", phase).await;
    let _ = set_setting(pool, "pomodoro_until", &until.to_rfc3339()).await;
}

pub fn start_pomodoro(
    app: tauri::AppHandle,
    work_mode: Arc<Mutex<WorkMode>>,
    pool: SqlitePool,
) -> tokio::sync::mpsc::UnboundedSender<PomodoroCmd> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PomodoroCmd>();

    tokio::spawn(async move {
        let mut in_study = false;
        // Ручной старт независим от Study: выставляется по PomodoroCmd::Start,
        // гасится только по Stop (выход из Study его не трогает).
        let mut manual = false;
        let mut working = true;
        let mut paused = false;
        let mut remaining: u64 = 25 * 60;
        let mut work_secs: u64 = 25 * 60;
        let mut break_secs: u64 = 5 * 60;

        loop {
            tokio::select! {
                _ = sleep(Duration::from_secs(1)) => {}
                Some(cmd) = rx.recv() => {
                    match cmd {
                        PomodoroCmd::Start => {
                            if in_study || manual { continue; }
                            manual = true;
                            working = true;
                            paused = false;
                            work_secs = get_u64_setting(&pool, "pomodoro_work_mins", 25).await.max(1) * 60;
                            break_secs = get_u64_setting(&pool, "pomodoro_break_mins", 5).await.max(1) * 60;
                            remaining = work_secs;
                            let until = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);
                            persist_state(&pool, "work", until).await;
                            if get_bool_setting(&pool, "focus_mode_auto", true).await {
                                crate::notifier::mute::extend_quiet_until(&pool, until).await;
                            }
                        }
                        PomodoroCmd::Stop => {
                            if !in_study && !manual { continue; }
                            in_study = false;
                            manual = false;
                            paused = false;
                            persist_state(&pool, "off", chrono::Utc::now()).await;
                        }
                        PomodoroCmd::TogglePause => {
                            if !in_study && !manual { continue; }
                            paused = !paused;
                            let until = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);
                            persist_state(&pool, if paused { "paused" } else if working { "work" } else { "break" }, until).await;
                        }
                        PomodoroCmd::Skip => {
                            if !in_study && !manual { continue; }
                            if working {
                                log_completed_work(&pool).await;
                            }
                            working = !working;
                            remaining = if working { work_secs } else { break_secs };
                            let until = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);
                            persist_state(&pool, if working { "work" } else { "break" }, until).await;
                            if working && get_bool_setting(&pool, "focus_mode_auto", true).await {
                                crate::notifier::mute::extend_quiet_until(&pool, until).await;
                            }
                        }
                    }
                    continue;
                }
            }

            let mode = work_mode.lock().unwrap().clone();
            if mode != WorkMode::Study {
                if in_study {
                    in_study = false;
                    if !manual {
                        paused = false;
                        persist_state(&pool, "off", chrono::Utc::now()).await;
                    }
                }
                if !manual { continue; }
            } else if !in_study && !manual {
                in_study = true;
                working = true;
                paused = false;
                // .max(1) — страховка от 0 в БД: иначе remaining -= 1 уйдёт в underflow
                work_secs = get_u64_setting(&pool, "pomodoro_work_mins", 25).await.max(1) * 60;
                break_secs = get_u64_setting(&pool, "pomodoro_break_mins", 5).await.max(1) * 60;
                remaining = work_secs;
                let until = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);
                persist_state(&pool, "work", until).await;
                if get_bool_setting(&pool, "focus_mode_auto", true).await {
                    crate::notifier::mute::extend_quiet_until(&pool, until).await;
                }
                // Пауза уведомлений: таймер идёт, но молча. Проверяем только в момент
                // отправки — не дёргаем БД каждый секундный тик.
                if !crate::notifier::mute::muted_now(&pool, &mode).await {
                    send_notification(&app, "Study", &format!("Помодоро запущено: {} минут работы", work_secs / 60));
                }
                continue;
            } else if !in_study && manual {
                // Study включился поверх уже идущего ручного цикла — считаем его "в Study"
                // для единообразия статуса, но цикл продолжается без перезапуска.
                in_study = true;
            }

            if paused {
                continue;
            }

            remaining -= 1;
            if remaining == 0 {
                let muted = crate::notifier::mute::muted_now(&pool, &mode).await;
                if working {
                    log_completed_work(&pool).await;
                    working = false;
                    remaining = break_secs;
                    if !muted { send_notification(&app, "Study", &format!("Перерыв {} минут — отдохни", break_secs / 60)); }
                } else {
                    working = true;
                    remaining = work_secs;
                    if !muted { send_notification(&app, "Study", &format!("Перерыв окончен: {} минут работы", work_secs / 60)); }
                }
                let until = chrono::Utc::now() + chrono::Duration::seconds(remaining as i64);
                persist_state(&pool, if working { "work" } else { "break" }, until).await;
                if working && get_bool_setting(&pool, "focus_mode_auto", true).await {
                    crate::notifier::mute::extend_quiet_until(&pool, until).await;
                }
            }
        }
    });

    tx
}
