use tauri::State;
use sqlx::{Row, SqlitePool};
use serde::Serialize;
use chrono::{DateTime, Utc};
use crate::error::AppResult;
pub use crate::notifier::pomodoro::{PomodoroCmd, PomodoroCmdTx};

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PomodoroState {
    pub phase: String, // "work" | "break" | "paused" | "off"
    pub until: Option<String>, // RFC3339, конец текущей фазы (для work/break/paused)
}

#[derive(Debug, Serialize, Clone, PartialEq, Default)]
pub struct PomodoroStats {
    pub today: i64,
    pub week: i64,
    pub task_streak: i64,
    pub pomodoro_streak: i64,
}

#[tauri::command]
pub async fn get_pomodoro_state(pool: State<'_, SqlitePool>) -> AppResult<PomodoroState> {
    get_pomodoro_state_impl(pool.inner()).await
}

pub async fn get_pomodoro_state_impl(pool: &SqlitePool) -> AppResult<PomodoroState> {
    let phase = crate::commands::settings::get_setting(pool, "pomodoro_phase")
        .await
        .unwrap_or_else(|| "off".into());
    let until = crate::commands::settings::get_setting(pool, "pomodoro_until").await;
    Ok(PomodoroState { phase, until })
}

#[tauri::command]
pub fn pomodoro_toggle_pause(tx: State<'_, PomodoroCmdTx>) {
    let _ = tx.0.send(PomodoroCmd::TogglePause);
}

#[tauri::command]
pub fn pomodoro_skip(tx: State<'_, PomodoroCmdTx>) {
    let _ = tx.0.send(PomodoroCmd::Skip);
}

#[tauri::command]
pub fn pomodoro_start(tx: State<'_, PomodoroCmdTx>) {
    let _ = tx.0.send(PomodoroCmd::Start);
}

#[tauri::command]
pub fn pomodoro_stop(tx: State<'_, PomodoroCmdTx>) {
    let _ = tx.0.send(PomodoroCmd::Stop);
}

#[tauri::command]
pub async fn get_pomodoro_stats(pool: State<'_, SqlitePool>) -> AppResult<PomodoroStats> {
    get_pomodoro_stats_impl(pool.inner(), Utc::now()).await
}

pub async fn get_pomodoro_stats_impl(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<PomodoroStats> {
    let today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pomodoro_log WHERE date(finished_at, 'localtime') = date(?, 'localtime')"
    )
    .bind(now.to_rfc3339())
    .fetch_one(pool)
    .await?;

    let week: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pomodoro_log WHERE date(finished_at, 'localtime') >= date(?, 'localtime', '-6 days')"
    )
    .bind(now.to_rfc3339())
    .fetch_one(pool)
    .await?;

    let task_streak = task_streak_impl(pool, now).await?;
    let pomodoro_streak = pomodoro_streak_impl(pool, now).await?;

    Ok(PomodoroStats { today, week, task_streak, pomodoro_streak })
}

// Дни подряд (от сегодня назад), где было ≥1 выполненной задачи. Сегодняшний
// день без выполненных задач не рвёт стрик — он мог просто ещё не закончиться.
async fn task_streak_impl(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<i64> {
    let rows = sqlx::query(
        "SELECT DISTINCT date(completed_at, 'localtime') as d FROM tasks WHERE completed_at IS NOT NULL"
    )
    .fetch_all(pool)
    .await?;
    let days: std::collections::HashSet<String> = rows.iter().map(|r| r.get::<String, _>("d")).collect();
    Ok(count_streak(&days, now))
}

// Дни подряд с ≥1 завершённым помидором, та же логика "сегодня не рвёт".
async fn pomodoro_streak_impl(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<i64> {
    let rows = sqlx::query(
        "SELECT DISTINCT date(finished_at, 'localtime') as d FROM pomodoro_log"
    )
    .fetch_all(pool)
    .await?;
    let days: std::collections::HashSet<String> = rows.iter().map(|r| r.get::<String, _>("d")).collect();
    Ok(count_streak(&days, now))
}

fn count_streak(days: &std::collections::HashSet<String>, now: DateTime<Utc>) -> i64 {
    let today_local = now.with_timezone(&chrono::Local).date_naive();
    let mut streak = 0i64;
    let mut cursor = today_local;
    loop {
        let key = cursor.format("%Y-%m-%d").to_string();
        if days.contains(&key) {
            streak += 1;
            cursor -= chrono::Duration::days(1);
        } else if cursor == today_local {
            // сегодня ещё не закончился — пропуск сегодняшнего дня не рвёт стрик,
            // но и не засчитывает его, продолжаем проверку со вчера
            cursor -= chrono::Duration::days(1);
        } else {
            break;
        }
    }
    streak
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_log(pool: &SqlitePool, finished_at: DateTime<Utc>, task_id: Option<&str>) {
        sqlx::query("INSERT INTO pomodoro_log (id, finished_at, task_id) VALUES (?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(finished_at.to_rfc3339())
            .bind(task_id)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn stats_today_and_week() {
        let pool = test_pool().await;
        let now = Utc::now();
        insert_log(&pool, now, None).await;
        insert_log(&pool, now - chrono::Duration::days(1), None).await;
        insert_log(&pool, now - chrono::Duration::days(10), None).await;

        let stats = get_pomodoro_stats_impl(&pool, now).await.unwrap();
        assert_eq!(stats.today, 1);
        assert_eq!(stats.week, 2);
    }

    #[tokio::test]
    async fn pomodoro_streak_breaks_on_gap() {
        let pool = test_pool().await;
        let now = Utc::now();
        insert_log(&pool, now, None).await;
        insert_log(&pool, now - chrono::Duration::days(1), None).await;
        // пропуск -2 дня рвёт стрик дальше
        insert_log(&pool, now - chrono::Duration::days(3), None).await;

        let streak = pomodoro_streak_impl(&pool, now).await.unwrap();
        assert_eq!(streak, 2);
    }

    #[tokio::test]
    async fn today_without_pomodoro_does_not_break_yesterday_streak() {
        let pool = test_pool().await;
        let now = Utc::now();
        // Ничего сегодня, но вчера и позавчера — есть
        insert_log(&pool, now - chrono::Duration::days(1), None).await;
        insert_log(&pool, now - chrono::Duration::days(2), None).await;

        let streak = pomodoro_streak_impl(&pool, now).await.unwrap();
        assert_eq!(streak, 2);
    }

    #[tokio::test]
    async fn task_streak_counts_completed_days() {
        let pool = test_pool().await;
        let now = Utc::now();
        for (id, offset) in [("t1", 0), ("t2", -1)] {
            sqlx::query(
                "INSERT INTO tasks (id, title, status, created_at, updated_at, completed_at) VALUES (?, 'x', 'Done', ?, ?, ?)"
            )
            .bind(id)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind((now + chrono::Duration::days(offset)).to_rfc3339())
            .execute(&pool).await.unwrap();
        }

        let streak = task_streak_impl(&pool, now).await.unwrap();
        assert_eq!(streak, 2);
    }
}
