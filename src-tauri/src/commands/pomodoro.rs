use tauri::State;
use sqlx::SqlitePool;
use serde::Serialize;
use crate::error::AppResult;
pub use crate::notifier::pomodoro::{PomodoroCmd, PomodoroCmdTx};

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PomodoroState {
    pub phase: String, // "work" | "break" | "paused" | "off"
    pub until: Option<String>, // RFC3339, конец текущей фазы (для work/break/paused)
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
