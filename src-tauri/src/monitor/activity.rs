use chrono::Utc;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::time::{interval, Duration};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ActivityState {
    Active,
    Idle,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    pub active_secs: u64,
    pub idle_secs: u64,
    pub session_start: String,
}

#[derive(Debug, Clone)]
pub struct ActivityTracker {
    pub state: Arc<Mutex<ActivityState>>,
    pub last_input: Arc<Mutex<chrono::DateTime<Utc>>>,
    pub session_start: chrono::DateTime<Utc>,
    pub active_secs: Arc<Mutex<u64>>,
    pub idle_secs: Arc<Mutex<u64>>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ActivityState::Active)),
            last_input: Arc::new(Mutex::new(Utc::now())),
            session_start: Utc::now(),
            active_secs: Arc::new(Mutex::new(0)),
            idle_secs: Arc::new(Mutex::new(0)),
        }
    }

    // Вызывается с фронта когда есть mousemove/keydown
    pub fn record_input(&self) {
        let mut last = self.last_input.lock().unwrap();
        *last = Utc::now();
        let mut state = self.state.lock().unwrap();
        *state = ActivityState::Active;
    }

    pub fn get_stats(&self) -> SessionStats {
        SessionStats {
            active_secs: *self.active_secs.lock().unwrap(),
            idle_secs: *self.idle_secs.lock().unwrap(),
            session_start: self.session_start.to_rfc3339(),
        }
    }

    pub fn get_state(&self) -> ActivityState {
        self.state.lock().unwrap().clone()
    }
}

pub fn start_activity_loop(
    tracker: Arc<ActivityTracker>,
    pool: SqlitePool,
    idle_threshold_secs: u64,
) {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(60));
        loop {
            tick.tick().await;

            let now = Utc::now();
            let last_input = *tracker.last_input.lock().unwrap();
            let elapsed = (now - last_input).num_seconds() as u64;

            // Обновляем состояние
            let new_state = if elapsed >= idle_threshold_secs {
                ActivityState::Idle
            } else {
                ActivityState::Active
            };

            {
                let mut state = tracker.state.lock().unwrap();
                *state = new_state.clone();
            }

            // Накапливаем статистику
            match new_state {
                ActivityState::Active => {
                    let mut secs = tracker.active_secs.lock().unwrap();
                    *secs += 60;
                }
                ActivityState::Idle => {
                    let mut secs = tracker.idle_secs.lock().unwrap();
                    *secs += 60;
                }
            }

            // Логируем в БД каждые 60 секунд
            let state_str = if elapsed >= idle_threshold_secs { "Idle" } else { "Active" };
            let result = sqlx::query(
                "INSERT INTO activity_log (timestamp, state, app_focused, input_events)
                 VALUES (?, ?, ?, ?)"
            )
            .bind(now.to_rfc3339())
            .bind(state_str)
            .bind(true)
            .bind(0i32)
            .execute(&pool)
            .await;

            let _ = result;
        }
    });
}

#[derive(serde::Serialize)]
pub struct ActivityDay {
    pub date: String,
    pub minutes: i64,
}

#[derive(serde::Serialize)]
pub struct TaskCompletion {
    pub date: String,
    pub completed: i64,
}