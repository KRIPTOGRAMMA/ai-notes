use tauri::State;
use std::sync::Arc;
use sqlx::{SqlitePool, Row};
use crate::error::AppResult;
use crate::monitor::activity::{ActivityTracker, SessionStats, ActivityState, ActivityDay, TaskCompletion, CategoryCount, ActiveIdleRatio};

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
#[tauri::command]
pub async fn get_category_distribution(pool: State<'_, SqlitePool>) -> AppResult<Vec<CategoryCount>> {
    get_category_distribution_impl(pool.inner()).await
}

pub async fn get_category_distribution_impl(pool: &SqlitePool) -> AppResult<Vec<CategoryCount>> {
    let rows = sqlx::query(
        "SELECT category, COUNT(*) as count
         FROM tasks
         WHERE completed_at IS NOT NULL
         GROUP BY category"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|row| CategoryCount {
        category: row.get("category"),
        count: row.get("count"),
    }).collect())
}

#[tauri::command]
pub async fn get_active_idle_ratio(pool: State<'_, SqlitePool>) -> AppResult<ActiveIdleRatio> {
    get_active_idle_ratio_impl(pool.inner()).await
}

pub async fn get_active_idle_ratio_impl(pool: &SqlitePool) -> AppResult<ActiveIdleRatio> {
    let (today_active, today_idle) =
        state_sums(pool, "date(timestamp) = date('now')").await?;
    let (week_active, week_idle) =
        state_sums(pool, "date(timestamp) >= date('now','-6 days')").await?;
    Ok(ActiveIdleRatio { today_active, today_idle, week_active, week_idle })
}

async fn state_sums(pool: &SqlitePool, window: &str) -> AppResult<(i64, i64)> {
    let sql = format!(
        "SELECT state, SUM(duration_secs) as secs FROM activity_log WHERE {} GROUP BY state",
        window
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    let (mut active, mut idle) = (0i64, 0i64);
    for row in &rows {
        let state: String = row.get("state");
        let secs: i64 = row.get("secs");
        match state.as_str() {
            "Active" => active = secs,
            "Idle" => idle = secs,
            _ => {}
        }
    }
    Ok((active, idle))
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

    async fn insert_task(pool: &SqlitePool, id: &str, category: &str, completed_at: Option<&str>) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, tags, recurrence, hidden, created_at, updated_at, completed_at)
             VALUES (?, 't', 'Done', 'Medium', ?, '[]', 'None', 0, '2026-07-01T00:00:00+00:00', '2026-07-01T00:00:00+00:00', ?)")
            .bind(id).bind(category).bind(completed_at)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn category_distribution_counts_only_completed() {
        let pool = test_pool().await;
        insert_task(&pool, "a", "Work", Some("2026-07-01T12:00:00+00:00")).await;
        insert_task(&pool, "b", "Work", Some("2026-07-02T12:00:00+00:00")).await;
        insert_task(&pool, "c", "Health", Some("2026-07-02T13:00:00+00:00")).await;
        insert_task(&pool, "d", "Study", None).await; // не выполнена — не считается

        let cats = get_category_distribution_impl(&pool).await.unwrap();
        assert_eq!(cats.len(), 2);
        let get = |name: &str| cats.iter().find(|c| c.category == name).map(|c| c.count);
        assert_eq!(get("Work"), Some(2));
        assert_eq!(get("Health"), Some(1));
        assert_eq!(get("Study"), None);
    }

    #[tokio::test]
    async fn active_idle_ratio_splits_today_and_week() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        let ts = |days_ago: i64| (now - chrono::Duration::days(days_ago)).to_rfc3339();

        // Сегодня: 120с актив + 60с простой
        log(&pool, &ts(0), "Active", 120).await;
        log(&pool, &ts(0), "Idle", 60).await;
        // 3 дня назад: попадает в неделю, но не в сегодня
        log(&pool, &ts(3), "Active", 300).await;
        // 10 дней назад: вне обоих окон
        log(&pool, &ts(10), "Active", 999).await;
        log(&pool, &ts(10), "Idle", 999).await;

        let r = get_active_idle_ratio_impl(&pool).await.unwrap();
        assert_eq!((r.today_active, r.today_idle), (120, 60));
        assert_eq!((r.week_active, r.week_idle), (420, 60));
    }
}
