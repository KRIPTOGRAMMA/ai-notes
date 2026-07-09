use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;
use tokio::time::{sleep, Duration};
use crate::commands::settings::{WorkMode, get_u64_setting};
use crate::notifier::scheduler::send_notification;

pub fn start_pomodoro(app: tauri::AppHandle, work_mode: Arc<Mutex<WorkMode>>, pool: SqlitePool) {
    tokio::spawn(async move {
        let mut in_study = false;
        let mut working = true;
        let mut remaining: u64 = 25 * 60;
        let mut work_secs: u64 = 25 * 60;
        let mut break_secs: u64 = 5 * 60;

        loop {
            sleep(Duration::from_secs(1)).await;

            let study = *work_mode.lock().unwrap() == WorkMode::Study;
            if !study {
                in_study = false;
                continue;
            }

            if !in_study {
                in_study = true;
                working = true;
                // .max(1) — страховка от 0 в БД: иначе remaining -= 1 уйдёт в underflow
                work_secs = get_u64_setting(&pool, "pomodoro_work_mins", 25).await.max(1) * 60;
                break_secs = get_u64_setting(&pool, "pomodoro_break_mins", 5).await.max(1) * 60;
                remaining = work_secs;
                send_notification(&app, "Study", &format!("Помодоро запущено: {} минут работы", work_secs / 60));
                continue;
            }

            remaining -= 1;
            if remaining == 0 {
                if working {
                    working = false;
                    remaining = break_secs;
                    send_notification(&app, "Study", &format!("Перерыв {} минут — отдохни", break_secs / 60));
                } else {
                    working = true;
                    remaining = work_secs;
                    send_notification(&app, "Study", &format!("Перерыв окончен: {} минут работы", work_secs / 60));
                }
            }
        }
    });
}
