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
    app: tauri::AppHandle,
    tracker: Arc<ActivityTracker>,
    pool: SqlitePool,
    idle_threshold_secs: u64,
    log_interval_secs: u64,
    work_mode: Arc<Mutex<crate::commands::settings::WorkMode>>,
) {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(log_interval_secs));
        // Локальное prev_state: tracker.state мгновенно сбрасывается в Active
        // из record_input, поэтому переход Idle→Active по нему не поймать.
        let mut prev_state = ActivityState::Active;
        let mut idle_since: Option<chrono::DateTime<Utc>> = None;
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

            // Контекстные уведомления: ловим переходы между состояниями
            if prev_state == ActivityState::Active && new_state == ActivityState::Idle {
                idle_since = Some(last_input);
            }
            if prev_state == ActivityState::Idle && new_state == ActivityState::Active {
                let away_mins = idle_since.map(|t| (now - t).num_minutes()).unwrap_or(0);
                idle_since = None;
                // Копируем режим в локальную переменную: держать lock через .await нельзя
                let focus = *work_mode.lock().unwrap() == crate::commands::settings::WorkMode::Focus;
                if !focus {
                    notify_return(&app, &pool, away_mins).await;
                }
            }
            prev_state = new_state.clone();

            // Накапливаем статистику
            match new_state {
                ActivityState::Active => {
                    let mut secs = tracker.active_secs.lock().unwrap();
                    *secs += log_interval_secs;
                }
                ActivityState::Idle => {
                    let mut secs = tracker.idle_secs.lock().unwrap();
                    *secs += log_interval_secs;
                }
            }

            // Логируем в БД каждые 60 секунд
            let state_str = if elapsed >= idle_threshold_secs { "Idle" } else { "Active" };
            let result = sqlx::query(
                "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs)
                 VALUES (?, ?, ?, ?, ?)"
            )
            .bind(now.to_rfc3339())
            .bind(state_str)
            .bind(true)
            .bind(0i32)
            .bind(log_interval_secs as i64)
            .execute(&pool)
            .await;

            let _ = result;
        }
    });
}

// Нотификация при возвращении после простоя: топ-задача = ближайший дедлайн,
// затем наивысший приоритет. Не шлём, если пользователь отходил ненадолго.
async fn notify_return(app: &tauri::AppHandle, pool: &SqlitePool, away_mins: i64) {
    let min_mins = crate::commands::settings::get_u64_setting(pool, "idle_notify_min_mins", 10).await;
    if away_mins < min_mins as i64 {
        return;
    }

    use sqlx::Row;
    let top_task: Option<String> = sqlx::query(
        "SELECT title FROM tasks
         WHERE status IN ('Todo', 'InProgress') AND hidden = 0
         ORDER BY deadline IS NULL, deadline ASC,
                  CASE priority
                      WHEN 'Critical' THEN 0
                      WHEN 'High' THEN 1
                      WHEN 'Medium' THEN 2
                      ELSE 3
                  END
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|row| row.get("title"));

    let body = match top_task {
        Some(title) => format!("Вы отсутствовали {} мин. Ближайшая задача: {}", away_mins, title),
        None => format!("Вы отсутствовали {} мин. С возвращением!", away_mins),
    };

    crate::notifier::scheduler::send_notification(app, "AI Notes", &body);
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