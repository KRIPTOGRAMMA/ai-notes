use tauri::State;
use std::sync::Arc;
use sqlx::{SqlitePool, Row};
use crate::monitor::activity::{ActivityTracker, SessionStats, ActivityState, ActivityDay};

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
pub async fn get_activity_by_day(pool: State<'_, SqlitePool>) -> Result<Vec<ActivityDay>, String> {
    let rows = sqlx::query(
        "SELECT date(timestamp) as date, COUNT(*) * 5 as minutes
         FROM activity_log
         WHERE state = 'Active'
         GROUP BY date(timestamp)"
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.iter().map(|row| ActivityDay {
        date: row.get("date"),
        minutes: row.get("minutes"),
    }).collect())
}