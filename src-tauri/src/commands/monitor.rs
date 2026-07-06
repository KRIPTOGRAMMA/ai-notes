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
    let rows = sqlx::query(
        "SELECT date(timestamp) as date, SUM(duration_secs) / 60 as minutes
         FROM activity_log
         WHERE state = 'Active'
         GROUP BY date(timestamp)"
    )
    .fetch_all(pool.inner())
    .await?;

    Ok(rows.iter().map(|row| ActivityDay {
        date: row.get("date"),
        minutes: row.get("minutes"),
    }).collect())
}

#[tauri::command]
pub async fn get_task_completions_by_day(pool: State<'_, SqlitePool>) -> AppResult<Vec<TaskCompletion>> {
    let rows = sqlx::query(
      "SELECT date(completed_at) as date, COUNT(*) as completed
       FROM tasks
       WHERE completed_at IS NOT NULL
       GROUP BY date(completed_at)"
    )
    .fetch_all(pool.inner())
    .await?;

    Ok(rows.iter().map(|row| TaskCompletion {
      date: row.get("date"),
      completed: row.get("completed"),
    }).collect())
}