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
    // Локальные сутки: completed_at хранится в UTC, а «день» для пользователя —
    // локальный (иначе вечерние задачи уезжают на завтра). Так же группирует
    // календарь-«квадратики» и get_completions_for_day.
    let rows = sqlx::query(
      "SELECT date(completed_at, 'localtime') as date, COUNT(*) as completed
       FROM tasks
       WHERE completed_at IS NOT NULL
       GROUP BY date(completed_at, 'localtime')"
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

// ===== Трекинг по приложениям (v0.5 фаза 1) =====

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct AppMinutes {
    pub app: String,
    pub minutes: i64,
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct CategoryMinutes {
    pub category: String,
    pub minutes: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct CategoryRule {
    pub pattern: String,
    pub category: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppLimit {
    pub category: String,
    pub daily_mins: i64, // 0/отсутствие правила = без лимита
}

const KNOWN_CATEGORIES: [&str; 5] = ["Work", "Study", "Home", "Health", "Other"];

// Лимиты времени на категории приложений: JSON в settings под ключом
// app_limits: [{"category":"Other","daily_mins":60}, ...]. Мусор/пустая строка — нет лимитов.
pub fn parse_app_limits(json: &str) -> Vec<AppLimit> {
    serde_json::from_str(json).unwrap_or_default()
}

// Правила категоризации приложений: JSON в settings под ключом
// app_category_rules: [{"pattern":"kitty","category":"Work"}, ...].
// Мусор/пустая строка — просто нет правил.
pub fn parse_category_rules(json: &str) -> Vec<CategoryRule> {
    serde_json::from_str(json).unwrap_or_default()
}

// Глоб с '*' (любая подстрока), регистронезависимый. Без '*' — точное совпадение.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.trim().to_lowercase();
    let t = text.to_lowercase();
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == t;
    }
    let mut pos = 0usize;
    let last = parts.len() - 1;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !t.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == last {
            return t.len() >= pos + part.len() && t[pos..].ends_with(part);
        } else {
            match t[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }
    true
}

// Первое совпавшее правило выигрывает; нет совпадений или неизвестная
// категория — "Other" (дашборд знает только 5 категорий палитры).
pub fn categorize_app(app: &str, rules: &[CategoryRule]) -> String {
    for rule in rules {
        if glob_match(&rule.pattern, app) && KNOWN_CATEGORIES.contains(&rule.category.as_str()) {
            return rule.category.clone();
        }
    }
    "Other".into()
}

async fn app_minutes_since(pool: &SqlitePool, days: i64) -> AppResult<Vec<AppMinutes>> {
    let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
    let rows = sqlx::query(
        "SELECT app, SUM(duration_secs) / 60 as minutes
         FROM activity_log
         WHERE state = 'Active' AND app IS NOT NULL AND timestamp >= ?
         GROUP BY app
         ORDER BY minutes DESC",
    )
    .bind(&since)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| AppMinutes { app: row.get("app"), minutes: row.get("minutes") })
        .collect())
}

#[tauri::command]
pub async fn get_app_usage(pool: State<'_, SqlitePool>, days: i64) -> AppResult<Vec<AppMinutes>> {
    get_app_usage_impl(pool.inner(), days).await
}

// Топ-10 приложений по активным минутам за последние N дней.
pub async fn get_app_usage_impl(pool: &SqlitePool, days: i64) -> AppResult<Vec<AppMinutes>> {
    let mut apps = app_minutes_since(pool, days.max(1)).await?;
    apps.truncate(10);
    Ok(apps)
}

#[tauri::command]
pub async fn get_app_category_time(
    pool: State<'_, SqlitePool>,
    days: i64,
) -> AppResult<Vec<CategoryMinutes>> {
    get_app_category_time_impl(pool.inner(), days).await
}

// Активные минуты по категориям: приложения из лога прогоняются через правила.
pub async fn get_app_category_time_impl(
    pool: &SqlitePool,
    days: i64,
) -> AppResult<Vec<CategoryMinutes>> {
    let rules_json = crate::commands::settings::get_setting(pool, "app_category_rules")
        .await
        .unwrap_or_default();
    let rules = parse_category_rules(&rules_json);

    let mut by_cat = std::collections::BTreeMap::<String, i64>::new();
    for row in app_minutes_since(pool, days.max(1)).await? {
        *by_cat.entry(categorize_app(&row.app, &rules)).or_default() += row.minutes;
    }

    let mut out: Vec<CategoryMinutes> = by_cat
        .into_iter()
        .map(|(category, minutes)| CategoryMinutes { category, minutes })
        .collect();
    out.sort_by(|a, b| b.minutes.cmp(&a.minutes));
    Ok(out)
}

// ===== Дашборд-аналитика (v0.6.5) =====

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct DayCompletion {
    pub id: String,
    pub title: String,
}

// Выполненные задачи конкретного локального дня (для попапа/тултипа календаря).
#[tauri::command]
pub async fn get_completions_for_day(pool: State<'_, SqlitePool>, date: String) -> AppResult<Vec<DayCompletion>> {
    get_completions_for_day_impl(pool.inner(), date).await
}

pub async fn get_completions_for_day_impl(pool: &SqlitePool, date: String) -> AppResult<Vec<DayCompletion>> {
    let rows = sqlx::query(
        "SELECT id, title FROM tasks
         WHERE completed_at IS NOT NULL AND date(completed_at, 'localtime') = ?
         ORDER BY completed_at",
    )
    .bind(&date)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| DayCompletion { id: r.get("id"), title: r.get("title") }).collect())
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub struct HourCell {
    pub weekday: i64, // 0 = воскресенье … 6 = суббота (strftime %w)
    pub hour: i64,    // 0–23, локальное время
    pub minutes: i64,
}

// Heatmap «час × день недели»: активные минуты за последние N дней.
#[tauri::command]
pub async fn get_hourly_activity(pool: State<'_, SqlitePool>, days: i64) -> AppResult<Vec<HourCell>> {
    get_hourly_activity_impl(pool.inner(), days).await
}

pub async fn get_hourly_activity_impl(pool: &SqlitePool, days: i64) -> AppResult<Vec<HourCell>> {
    let since = format!("-{} days", days.max(1));
    let rows = sqlx::query(
        "SELECT CAST(strftime('%w', timestamp, 'localtime') AS INTEGER) AS weekday,
                CAST(strftime('%H', timestamp, 'localtime') AS INTEGER) AS hour,
                SUM(duration_secs) / 60 AS minutes
         FROM activity_log
         WHERE state = 'Active' AND date(timestamp) >= date('now', ?)
         GROUP BY weekday, hour",
    )
    .bind(&since)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| HourCell { weekday: r.get("weekday"), hour: r.get("hour"), minutes: r.get("minutes") })
        .collect())
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

    #[tokio::test]
    async fn completions_for_day_returns_titles_of_local_day() {
        use chrono::{Local, TimeZone, Duration};
        let pool = test_pool().await;

        let today_noon = Local::now().date_naive().and_hms_opt(12, 0, 0).unwrap();
        let today_utc = Local.from_local_datetime(&today_noon).single().unwrap().to_utc();
        for (id, title, at) in [
            ("d1", "сегодняшняя", today_utc),
            ("d2", "вчерашняя", today_utc - Duration::days(1)),
        ] {
            sqlx::query(
                "INSERT INTO tasks (id, title, status, priority, category, tags, recurrence, hidden, created_at, updated_at, completed_at)
                 VALUES (?, ?, 'Done', 'Medium', 'Work', '[]', 'None', 1, ?, ?, ?)")
                .bind(id).bind(title)
                .bind(at.to_rfc3339()).bind(at.to_rfc3339()).bind(at.to_rfc3339())
                .execute(&pool).await.unwrap();
        }

        let key = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let completions = get_completions_for_day_impl(&pool, key).await.unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].id, "d1");
        assert_eq!(completions[0].title, "сегодняшняя");
        assert!(get_completions_for_day_impl(&pool, "1999-01-01".into()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn hourly_activity_groups_by_local_hour_and_window() {
        use chrono::{Datelike, Timelike, Duration, Local, Utc};
        let pool = test_pool().await;

        // Стабильный момент внутри часа: -3ч от «сейчас», минута 10
        let t = (Utc::now() - Duration::hours(3))
            .with_minute(10).unwrap()
            .with_second(0).unwrap()
            .with_nanosecond(0).unwrap();
        log(&pool, &t.to_rfc3339(), "Active", 600).await; // 10 мин
        log(&pool, &(t + Duration::minutes(5)).to_rfc3339(), "Active", 300).await; // +5 мин, тот же час
        log(&pool, &t.to_rfc3339(), "Idle", 600).await; // не считается
        log(&pool, &(Utc::now() - Duration::days(100)).to_rfc3339(), "Active", 600).await; // вне окна

        let cells = get_hourly_activity_impl(&pool, 7).await.unwrap();
        let local = t.with_timezone(&Local);
        assert_eq!(cells, vec![HourCell {
            weekday: local.weekday().num_days_from_sunday() as i64,
            hour: local.hour() as i64,
            minutes: 15,
        }]);
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

    #[test]
    fn glob_match_cases() {
        assert!(glob_match("kitty", "kitty"));
        assert!(glob_match("KiTTy", "kitty")); // регистр не важен
        assert!(!glob_match("kitty", "kitty-extra")); // без '*' — точное
        assert!(glob_match("kitty*", "kitty-extra"));
        assert!(glob_match("*fox", "firefox"));
        assert!(glob_match("*ire*", "firefox"));
        assert!(glob_match("jetbrains-*", "jetbrains-idea"));
        assert!(!glob_match("jetbrains-*", "idea-jetbrains"));
        assert!(glob_match("*", "что угодно"));
        assert!(!glob_match("a*b", "ba")); // порядок частей обязателен
    }

    #[test]
    fn categorize_first_match_wins_and_unknown_is_other() {
        let rules = parse_category_rules(
            r#"[{"pattern":"jetbrains-*","category":"Work"},
                {"pattern":"*","category":"Study"},
                {"pattern":"zen","category":"Игры"}]"#,
        );
        assert_eq!(categorize_app("jetbrains-idea", &rules), "Work");
        assert_eq!(categorize_app("kitty", &rules), "Study"); // wildcard-правило
        // «Игры» — не из палитры: правило пропускается (здесь ловит wildcard)
        assert_eq!(categorize_app("zen", &rules), "Study");

        assert_eq!(categorize_app("anything", &[]), "Other");
        assert!(parse_category_rules("мусор").is_empty());
        assert!(parse_category_rules("").is_empty());
    }

    async fn log_app(pool: &SqlitePool, ts: &str, app: Option<&str>, duration_secs: i64) {
        sqlx::query(
            "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs, app)
             VALUES (?, 'Active', 1, 0, ?, ?)")
            .bind(ts).bind(duration_secs).bind(app)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn app_usage_sums_and_respects_window() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        let ts = |days_ago: i64| (now - chrono::Duration::days(days_ago)).to_rfc3339();

        log_app(&pool, &ts(0), Some("kitty"), 600).await;
        log_app(&pool, &ts(0), Some("kitty"), 600).await;
        log_app(&pool, &ts(0), Some("zen"), 300).await;
        log_app(&pool, &ts(0), None, 999).await; // без app — не считается
        log_app(&pool, &ts(30), Some("kitty"), 6000).await; // вне окна

        let usage = get_app_usage_impl(&pool, 7).await.unwrap();
        assert_eq!(usage[0], AppMinutes { app: "kitty".into(), minutes: 20 });
        assert_eq!(usage[1], AppMinutes { app: "zen".into(), minutes: 5 });
        assert_eq!(usage.len(), 2);
    }

    #[tokio::test]
    async fn category_time_applies_rules_from_settings() {
        let pool = test_pool().await;
        crate::commands::settings::set_setting(
            &pool,
            "app_category_rules",
            r#"[{"pattern":"kitty","category":"Work"}]"#,
        )
        .await
        .unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        log_app(&pool, &now, Some("kitty"), 600).await;
        log_app(&pool, &now, Some("zen"), 300).await; // нет правила → Other

        let cats = get_app_category_time_impl(&pool, 1).await.unwrap();
        assert_eq!(cats[0], CategoryMinutes { category: "Work".into(), minutes: 10 });
        assert_eq!(cats[1], CategoryMinutes { category: "Other".into(), minutes: 5 });
    }
}
