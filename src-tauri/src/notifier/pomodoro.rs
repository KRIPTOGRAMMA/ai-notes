use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};
use crate::commands::settings::WorkMode;
use crate::notifier::scheduler::send_notification;

const WORK_SECS: u64 = 25 * 60;
const BREAK_SECS: u64 = 5 * 60;

// Помодоро-цикл для режима Study: 25 минут работы / 5 минут перерыва.
// Живёт постоянно, но тикает только пока выбран Study; при выходе из
// режима цикл сбрасывается и при возврате начинается заново с работы.
pub fn start_pomodoro(app: tauri::AppHandle, work_mode: Arc<Mutex<WorkMode>>) {
    tokio::spawn(async move {
        let mut in_study = false;
        let mut working = true;
        let mut remaining = WORK_SECS;

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
                remaining = WORK_SECS;
                send_notification(&app, "Study", "Помодоро запущено: 25 минут работы");
                continue;
            }

            remaining -= 1;
            if remaining == 0 {
                if working {
                    working = false;
                    remaining = BREAK_SECS;
                    send_notification(&app, "Study", "Перерыв 5 минут — отдохни");
                } else {
                    working = true;
                    remaining = WORK_SECS;
                    send_notification(&app, "Study", "Перерыв окончен: 25 минут работы");
                }
            }
        }
    });
}
