use std::sync::{Arc, Mutex};
use std::time::Instant;
use sqlx::{SqlitePool, Row};
use tokio::time::{sleep, Duration};
use crate::commands::settings::{WorkMode, get_u64_setting};
use crate::notifier::scheduler::send_notification;

const CHECK_EVERY_SECS: u64 = 300; // раз в 5 минут
const LOOKBACK_HOURS: i64 = 4;     // сколько истории смотрим на «непрерывный хвост»

// Чистая логика: непрерывный «активный хвост» в секундах. Идём с конца
// (самые свежие записи) и суммируем Active, пока не встретим Idle — она
// обрывает серию. rows отсортированы по времени по возрастанию.
pub fn trailing_active_secs(rows: &[(String, i64)]) -> i64 {
    let mut total = 0;
    for (state, dur) in rows.iter().rev() {
        if state == "Active" {
            total += *dur;
        } else {
            break;
        }
    }
    total
}

// Мягкие напоминания о перерыве: если пользователь непрерывно активен дольше
// порога — предлагаем размяться. Работает только в Light (в Focus всё заглушено,
// в Study уже есть помодоро). Порог `nudge_after_mins` из настроек, 0 — выкл.
pub fn start_nudger(app: tauri::AppHandle, pool: SqlitePool, work_mode: Arc<Mutex<WorkMode>>) {
    tokio::spawn(async move {
        let mut last_nudge: Option<Instant> = None;
        loop {
            sleep(Duration::from_secs(CHECK_EVERY_SECS)).await;

            if *work_mode.lock().unwrap() != WorkMode::Light {
                continue;
            }

            let after_mins = get_u64_setting(&pool, "nudge_after_mins", 90).await;
            if after_mins == 0 {
                continue;
            }

            // Кулдаун: не чаще раза в половину порога (но не реже 20 мин),
            // иначе будем напоминать каждые 5 минут, пока человек работает.
            let cooldown = Duration::from_secs((after_mins * 60 / 2).max(20 * 60));
            if last_nudge.map(|t| t.elapsed() < cooldown).unwrap_or(false) {
                continue;
            }

            let cutoff = (chrono::Utc::now() - chrono::Duration::hours(LOOKBACK_HOURS)).to_rfc3339();
            let rows: Vec<(String, i64)> = match sqlx::query(
                "SELECT state, duration_secs FROM activity_log
                 WHERE timestamp >= ? ORDER BY timestamp ASC"
            )
            .bind(&cutoff)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows.into_iter()
                    .map(|r| (r.get::<String, _>("state"), r.get::<i64, _>("duration_secs")))
                    .collect(),
                Err(_) => continue,
            };

            let active_secs = trailing_active_secs(&rows);
            if active_secs >= (after_mins * 60) as i64 {
                send_notification(
                    &app,
                    "Пора сделать перерыв",
                    &format!("Ты работаешь без перерыва уже {} мин. Встань, разомнись :)", active_secs / 60),
                );
                last_nudge = Some(Instant::now());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(state: &str, dur: i64) -> (String, i64) {
        (state.to_string(), dur)
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(trailing_active_secs(&[]), 0);
    }

    #[test]
    fn sums_only_trailing_active() {
        let rows = vec![r("Active", 60), r("Idle", 60), r("Active", 60), r("Active", 60)];
        // хвост: две Active по 60 → 120; предыдущая Idle обрывает
        assert_eq!(trailing_active_secs(&rows), 120);
    }

    #[test]
    fn idle_at_end_breaks_immediately() {
        let rows = vec![r("Active", 60), r("Active", 60), r("Idle", 60)];
        assert_eq!(trailing_active_secs(&rows), 0);
    }

    #[test]
    fn all_active_sums_all() {
        let rows = vec![r("Active", 90), r("Active", 90), r("Active", 90)];
        assert_eq!(trailing_active_secs(&rows), 270);
    }
}
