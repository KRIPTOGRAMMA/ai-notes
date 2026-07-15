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

// Результат одного тика конечного автомата простоя. Чистые данные — без БД,
// уведомлений и блокировок, чтобы логику можно было покрыть юнит-тестами.
#[derive(Debug, Clone, PartialEq)]
pub struct IdleTick {
    pub state: ActivityState,
    pub idle_since: Option<chrono::DateTime<Utc>>,
    // Some(минуты) — если это переход Idle→Active и стоит подумать об уведомлении
    pub notify_return_mins: Option<i64>,
}

// Чистая логика одного тика: по предыдущему состоянию и таймингу считает новое
// состояние, момент начала простоя и надо ли уведомить о возвращении.
pub fn step_idle(
    prev_state: &ActivityState,
    idle_since: Option<chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
    last_input: chrono::DateTime<Utc>,
    idle_threshold_secs: u64,
) -> IdleTick {
    let elapsed = (now - last_input).num_seconds().max(0) as u64;
    let new_state = if elapsed >= idle_threshold_secs {
        ActivityState::Idle
    } else {
        ActivityState::Active
    };

    let mut new_idle_since = idle_since;
    let mut notify_return_mins = None;

    if *prev_state == ActivityState::Active && new_state == ActivityState::Idle {
        new_idle_since = Some(last_input);
    }
    if *prev_state == ActivityState::Idle && new_state == ActivityState::Active {
        let away = new_idle_since.map(|t| (now - t).num_minutes()).unwrap_or(0);
        notify_return_mins = Some(away);
        new_idle_since = None;
    }

    IdleTick { state: new_state, idle_since: new_idle_since, notify_return_mins }
}

pub fn start_activity_loop(
    app: tauri::AppHandle,
    tracker: Arc<ActivityTracker>,
    pool: SqlitePool,
    idle_threshold_secs: u64,
    log_interval_secs: u64,
    work_mode: Arc<Mutex<crate::commands::settings::WorkMode>>,
    window_provider: Option<Arc<dyn super::window::WindowProvider>>,
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

            let step = step_idle(&prev_state, idle_since, now, last_input, idle_threshold_secs);
            let new_state = step.state.clone();
            idle_since = step.idle_since;

            {
                let mut state = tracker.state.lock().unwrap();
                *state = new_state.clone();
            }

            // Переход Idle→Active: уведомление о возвращении (кроме Focus/паузы)
            if let Some(away_mins) = step.notify_return_mins {
                // Копируем режим в локальную переменную: держать lock через .await нельзя
                let mode = work_mode.lock().unwrap().clone();
                if !crate::notifier::mute::muted_now(&pool, &mode).await {
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

            // Логируем в БД каждый тик. Для Active-тика фиксируем класс окна
            // в фокусе (если провайдер есть): локальный сокет, цена нулевая.
            let state_str = match new_state {
                ActivityState::Idle => "Idle",
                ActivityState::Active => "Active",
            };
            let focused_app = match (&new_state, &window_provider) {
                (ActivityState::Active, Some(p)) => p.current_window().map(|w| w.app),
                _ => None,
            };
            let result = sqlx::query(
                "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs, app)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(now.to_rfc3339())
            .bind(state_str)
            .bind(true)
            .bind(0i32)
            .bind(log_interval_secs as i64)
            .bind(focused_app)
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

    let context_on = crate::commands::settings::get_setting(pool, "context_notifications")
        .await
        .as_deref()
        != Some("false");

    // Контекстный триггер (§5.4): долго отсутствовал и есть задача «в работе» —
    // любая InProgress, не обязательно топовая по дедлайну.
    let in_progress = if context_on && away_mins >= CONTEXT_RETURN_MINS {
        nearest_task(pool, &["InProgress"]).await
    } else {
        None
    };

    let body = match in_progress {
        Some(title) => {
            format!("Вы отсутствовали {} мин. Продолжим задачу «{}» или сделаем перерыв?", away_mins, title)
        }
        None => match nearest_task(pool, &["Todo", "InProgress"]).await {
            Some(title) => format!("Вы отсутствовали {} мин. Ближайшая задача: {}", away_mins, title),
            None => format!("Вы отсутствовали {} мин. С возвращением!", away_mins),
        },
    };

    crate::notifier::scheduler::send_notification(app, "AI Notes", &body);
}

// Ближайшая (по дедлайну, затем по приоритету) видимая задача с одним из
// заданных статусов. Статусы — фиксированные строки из кода, не ввод пользователя.
pub async fn nearest_task(pool: &SqlitePool, statuses: &[&str]) -> Option<String> {
    use sqlx::Row;
    let placeholders = vec!["?"; statuses.len()].join(", ");
    let sql = format!(
        "SELECT title FROM tasks
         WHERE status IN ({placeholders}) AND hidden = 0
         ORDER BY deadline IS NULL, deadline ASC,
                  CASE priority
                      WHEN 'Critical' THEN 0
                      WHEN 'High' THEN 1
                      WHEN 'Medium' THEN 2
                      ELSE 3
                  END
         LIMIT 1"
    );
    let mut query = sqlx::query(&sql);
    for s in statuses {
        query = query.bind(*s);
    }
    query
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get("title"))
}

// Порог «долгого» отсутствия для контекстного сообщения про InProgress-задачу.
const CONTEXT_RETURN_MINS: i64 = 40;

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

#[derive(serde::Serialize)]
pub struct CategoryCount {
    pub category: String,
    pub count: i64,
}

#[derive(serde::Serialize)]
pub struct ActiveIdleRatio {
    pub today_active: i64,
    pub today_idle: i64,
    pub week_active: i64,
    pub week_idle: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn at(now: chrono::DateTime<Utc>, secs_ago: i64) -> chrono::DateTime<Utc> {
        now - ChronoDuration::seconds(secs_ago)
    }

    #[test]
    fn active_stays_active_below_threshold() {
        let now = Utc::now();
        let step = step_idle(&ActivityState::Active, None, now, at(now, 100), 300);
        assert_eq!(step.state, ActivityState::Active);
        assert_eq!(step.idle_since, None);
        assert_eq!(step.notify_return_mins, None);
    }

    #[test]
    fn threshold_is_inclusive_boundary() {
        let now = Utc::now();
        // ровно порог → Idle (>=)
        let step = step_idle(&ActivityState::Active, None, now, at(now, 300), 300);
        assert_eq!(step.state, ActivityState::Idle);
        // на секунду меньше порога → всё ещё Active
        let step = step_idle(&ActivityState::Active, None, now, at(now, 299), 300);
        assert_eq!(step.state, ActivityState::Active);
    }

    #[test]
    fn active_to_idle_records_idle_since_and_does_not_notify() {
        let now = Utc::now();
        let last_input = at(now, 400);
        let step = step_idle(&ActivityState::Active, None, now, last_input, 300);
        assert_eq!(step.state, ActivityState::Idle);
        assert_eq!(step.idle_since, Some(last_input));
        assert_eq!(step.notify_return_mins, None);
    }

    #[test]
    fn idle_stays_idle_keeps_idle_since() {
        let now = Utc::now();
        let idle_since = at(now, 600);
        let step = step_idle(&ActivityState::Idle, Some(idle_since), now, at(now, 500), 300);
        assert_eq!(step.state, ActivityState::Idle);
        assert_eq!(step.idle_since, Some(idle_since));
        assert_eq!(step.notify_return_mins, None);
    }

    #[test]
    fn idle_to_active_notifies_with_away_minutes_and_clears_idle_since() {
        let now = Utc::now();
        // ушёл в простой 30 минут назад, только что вернулся (last_input = сейчас)
        let idle_since = at(now, 30 * 60);
        let step = step_idle(&ActivityState::Idle, Some(idle_since), now, now, 300);
        assert_eq!(step.state, ActivityState::Active);
        assert_eq!(step.idle_since, None);
        assert_eq!(step.notify_return_mins, Some(30));
    }

    #[test]
    fn idle_to_active_without_idle_since_reports_zero() {
        let now = Utc::now();
        // idle_since не выставлен (крайний случай) — away = 0, но уведомление рассматривается
        let step = step_idle(&ActivityState::Idle, None, now, now, 300);
        assert_eq!(step.notify_return_mins, Some(0));
    }

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_task(pool: &sqlx::SqlitePool, title: &str, status: &str, deadline: Option<&str>) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, deadline, recurrence, tags, hidden, created_at, updated_at)
             VALUES (?, ?, ?, 'Medium', 'Work', ?, 'None', '[]', 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(title).bind(status).bind(deadline)
            .execute(pool).await.unwrap();
    }

    // Регресс: контекстный триггер должен находить задачу «в работе», даже если
    // топовая по дедлайну — Todo (раньше проверялся только статус топовой).
    #[tokio::test]
    async fn nearest_task_finds_in_progress_behind_todo_with_nearer_deadline() {
        let pool = test_pool().await;
        insert_task(&pool, "срочная todo", "Todo", Some("2026-07-15T00:00:00+00:00")).await;
        insert_task(&pool, "в работе без дедлайна", "InProgress", None).await;

        // топовая среди всех — Todo с ближайшим дедлайном
        assert_eq!(
            nearest_task(&pool, &["Todo", "InProgress"]).await.as_deref(),
            Some("срочная todo")
        );
        // но InProgress-задача находится отдельным запросом
        assert_eq!(
            nearest_task(&pool, &["InProgress"]).await.as_deref(),
            Some("в работе без дедлайна")
        );
    }

    #[tokio::test]
    async fn nearest_task_none_when_no_matching_status() {
        let pool = test_pool().await;
        insert_task(&pool, "выполнена", "Done", None).await;
        assert_eq!(nearest_task(&pool, &["InProgress"]).await, None);
    }
}