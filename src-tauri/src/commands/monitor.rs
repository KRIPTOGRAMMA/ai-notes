use tauri::State;
use std::sync::Arc;
use sqlx::{SqlitePool, Row};
use crate::error::AppResult;
use crate::monitor::activity::{ActivityTracker, SessionStats, ActivityState, ActivityDay, TaskCompletion};

#[tauri::command]
pub fn record_input(tracker: State<'_, Arc<ActivityTracker>>) {
    tracker.record_input();
}

#[tauri::command]
pub fn get_session_stats(tracker: State<'_, Arc<ActivityTracker>>) -> SessionStats {
    tracker.get_stats()
}

#[tauri::command]
pub fn get_activity_state(tracker: State<'_, Arc<ActivityTracker>>) -> String {
    match tracker.get_state() {
        ActivityState::Active => "Active".into(),
        ActivityState::Idle => "Idle".into(),
    }
}

#[tauri::command]
pub async fn get_activity_by_day(pool: State<'_, SqlitePool>) -> AppResult<Vec<ActivityDay>> {
    get_activity_by_day_impl(pool.inner()).await
}

pub async fn get_activity_by_day_impl(pool: &SqlitePool) -> AppResult<Vec<ActivityDay>> {
    let rows = sqlx::query(
        "SELECT date(timestamp) as date, SUM(duration_secs) / 60 as minutes
         FROM activity_log
         WHERE state = 'Active'
         GROUP BY date(timestamp)"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|row| ActivityDay {
        date: row.get("date"),
        minutes: row.get("minutes"),
    }).collect())
}

#[tauri::command]
pub async fn get_task_completions_by_day(pool: State<'_, SqlitePool>) -> AppResult<Vec<TaskCompletion>> {
    get_task_completions_by_day_impl(pool.inner()).await
}

pub async fn get_task_completions_by_day_impl(pool: &SqlitePool) -> AppResult<Vec<TaskCompletion>> {
    let rows = sqlx::query(
      "SELECT date(completed_at) as date, COUNT(*) as completed
       FROM tasks
       WHERE completed_at IS NOT NULL
       GROUP BY date(completed_at)"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|row| TaskCompletion {
      date: row.get("date"),
      completed: row.get("completed"),
    }).collect())
}
#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn log(pool: &SqlitePool, ts: &str, state: &str, duration_secs: i64) {
        sqlx::query(
            "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs)
             VALUES (?, ?, 1, 0, ?)")
            .bind(ts).bind(state).bind(duration_secs)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn activity_minutes_sum_durations_per_day() {
        let pool = test_pool().await;
        // День 1: 3 активных тика по 60с + idle (не считается)
        log(&pool, "2026-07-01T10:00:00+00:00", "Active", 60).await;
        log(&pool, "2026-07-01T10:01:00+00:00", "Active", 60).await;
        log(&pool, "2026-07-01T10:02:00+00:00", "Active", 60).await;
        log(&pool, "2026-07-01T10:03:00+00:00", "Idle", 60).await;
        // День 2: тики с другим интервалом (настройка сменилась) — 90с + 30с
        log(&pool, "2026-07-02T09:00:00+00:00", "Active", 90).await;
        log(&pool, "2026-07-02T09:02:00+00:00", "Active", 30).await;

        let days = get_activity_by_day_impl(&pool).await.unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, "2026-07-01");
        assert_eq!(days[0].minutes, 3);   // 180с / 60, Idle не учтён
        assert_eq!(days[1].date, "2026-07-02");
        assert_eq!(days[1].minutes, 2);   // (90+30)с / 60
    }

    #[tokio::test]
    async fn completions_grouped_by_day() {
        let pool = test_pool().await;
        for (id, day) in [("a", "01"), ("b", "01"), ("c", "02")] {
            sqlx::query(
                "INSERT INTO tasks (id, title, status, priority, category, tags, recurrence, hidden, created_at, updated_at, completed_at)
                 VALUES (?, 't', 'Done', 'Medium', 'Work', '[]', 'None', 1, '2026-07-01T00:00:00+00:00', '2026-07-01T00:00:00+00:00', ?)")
                .bind(id)
                .bind(format!("2026-07-{}T12:00:00+00:00", day))
                .execute(&pool).await.unwrap();
        }

        let days = get_task_completions_by_day_impl(&pool).await.unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!((days[0].date.as_str(), days[0].completed), ("2026-07-01", 2));
        assert_eq!((days[1].date.as_str(), days[1].completed), ("2026-07-02", 1));
    }
}
