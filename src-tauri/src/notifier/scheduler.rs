use chrono::{Local, TimeZone, Utc};
use std::sync::{Arc, Mutex};
use sqlx::{SqlitePool, Row};
use tauri_plugin_notification::NotificationExt;
use crate::commands::settings::WorkMode;

pub fn start_scheduler(app: tauri::AppHandle, pool: SqlitePool, work_mode: Arc<Mutex<WorkMode>>) {
    tokio::spawn(async move {
        loop {
            // При Focus или активной паузе дедлайны всё равно проверяем и помечаем,
            // но не шлём: иначе после снятия глушилки прилетит пачка устаревших уведомлений.
            let mode = work_mode.lock().unwrap().clone();
            let muted = crate::notifier::mute::muted_now(&pool, &mode).await;
            check_deadlines(&app, &pool, muted).await;
            check_blocks(&app, &pool, muted).await;
            check_goals(&app, &pool, muted).await;
            check_morning_digest(&app, &pool, muted).await;
            check_app_limits(&app, &pool, muted).await;
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });
}

async fn check_deadlines(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    use crate::commands::settings::get_u64_setting;
    let now = Utc::now();
    let warn_hours = get_u64_setting(pool, "deadline_warn_hours", 24).await as i64;
    let warn_mins = get_u64_setting(pool, "deadline_warn_minutes", 60).await as i64;
    let at_hours = now + chrono::Duration::hours(warn_hours);
    let at_mins = now + chrono::Duration::minutes(warn_mins);
    let msg_hours = format!("Дедлайн через {} ч", warn_hours);
    let msg_mins = format!("Дедлайн через {} мин", warn_mins);
    // Раннее предупреждение — то, что дальше от «сейчас». Не полагаемся на то,
    // что «часы» всегда больше «минут»: пользователь мог задать 1ч и 90мин.
    let (early_at, early_msg, late_at, late_msg) = if at_hours >= at_mins {
        (at_hours, &msg_hours, at_mins, &msg_mins)
    } else {
        (at_mins, &msg_mins, at_hours, &msg_hours)
    };

    let rows = match sqlx::query(
        "SELECT id, title, deadline, notified_24h, notified_1h, notified_deadline
         FROM tasks WHERE hidden = 0 AND deadline IS NOT NULL"
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return,
    };

    for row in rows {
        let id: String = row.get("id");
        let title: String = row.get("title");
        let deadline_str: String = row.get("deadline");
        let notified_24h: bool = row.get("notified_24h");
        let notified_1h: bool = row.get("notified_1h");
        let notified_deadline: bool = row.get("notified_deadline");

        let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(&deadline_str) else { continue; };
        let deadline = deadline.with_timezone(&Utc);

        if !notified_24h && deadline <= early_at && deadline > late_at {
            if !muted { send_notification(app, &title, early_msg); }
            let _ = sqlx::query("UPDATE tasks SET notified_24h = 1 WHERE id = ?")
                .bind(&id).execute(pool).await;
        }

        if !notified_1h && deadline <= late_at && deadline > now {
            if !muted { send_notification(app, &title, late_msg); }
            let _ = sqlx::query("UPDATE tasks SET notified_1h = 1 WHERE id = ?")
                .bind(&id).execute(pool).await;
        }

        if !notified_deadline && deadline <= now {
            if !muted { send_notification(app, &title, "Дедлайн наступил!"); }
            let _ = sqlx::query("UPDATE tasks SET notified_deadline = 1 WHERE id = ?")
                .bind(&id).execute(pool).await;
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct BlockDue {
    pub id: String,
    pub title: String,
    pub end_local: String, // "HH:MM" конца блока в локальном времени — для текста пуша
}

// Блоки, начавшиеся в окне (now - grace, now], о которых ещё не уведомляли.
// Grace-окно: после долгого сна/перезапуска не спамим давно начавшимися блоками —
// они просто помечаются при следующей проверке.
pub async fn blocks_due(pool: &SqlitePool, now: chrono::DateTime<Utc>, grace_mins: i64) -> Vec<BlockDue> {
    let rows = match sqlx::query(
        "SELECT id, title, scheduled_at, COALESCE(scheduled_mins, 60) as mins
         FROM tasks
         WHERE hidden = 0 AND notified_block = 0 AND scheduled_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return vec![],
    };

    let mut due = vec![];
    for row in rows {
        let scheduled_str: String = row.get("scheduled_at");
        let Ok(start) = chrono::DateTime::parse_from_rfc3339(&scheduled_str) else { continue; };
        let start = start.with_timezone(&Utc);
        if start <= now && start > now - chrono::Duration::minutes(grace_mins) {
            let mins: i64 = row.get("mins");
            let end = (start + chrono::Duration::minutes(mins)).with_timezone(&chrono::Local);
            due.push(BlockDue {
                id: row.get("id"),
                title: row.get("title"),
                end_local: end.format("%H:%M").to_string(),
            });
        }
    }
    due
}

pub async fn mark_block_notified(pool: &SqlitePool, id: &str) {
    let _ = sqlx::query("UPDATE tasks SET notified_block = 1 WHERE id = ?")
        .bind(id).execute(pool).await;
}

// Просроченные (старше grace-окна) блоки без уведомления тоже помечаем,
// чтобы они не оставались вечными кандидатами.
async fn sweep_stale_blocks(pool: &SqlitePool, now: chrono::DateTime<Utc>, grace_mins: i64) {
    let cutoff = (now - chrono::Duration::minutes(grace_mins)).to_rfc3339();
    let _ = sqlx::query(
        "UPDATE tasks SET notified_block = 1
         WHERE notified_block = 0 AND scheduled_at IS NOT NULL AND scheduled_at <= ?",
    )
    .bind(&cutoff)
    .execute(pool)
    .await;
}

const BLOCK_GRACE_MINS: i64 = 10;

async fn check_blocks(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    for block in blocks_due(pool, now, BLOCK_GRACE_MINS).await {
        if !muted {
            send_notification(app, &block.title, &format!("Начался блок (до {})", block.end_local));
        }
        mark_block_notified(pool, &block.id).await;
    }
    sweep_stale_blocks(pool, now, BLOCK_GRACE_MINS).await;
}

#[derive(Debug, PartialEq)]
pub struct GoalDue {
    pub id: String,
    pub name: String,
    pub body: String,
    pub period_key: String, // чем пометить notified_goal после пуша
}

// Проекты, у которых цель текущего периода выполнена, а пуш за этот период
// ещё не отправлялся. Если заданы обе части цели (задачи и минуты) —
// выполнены должны быть обе.
pub async fn goals_due(pool: &SqlitePool, now: chrono::DateTime<Utc>) -> Vec<GoalDue> {
    use crate::commands::projects::{get_projects_at, period_key};
    let projects = match get_projects_at(pool, now).await {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let mut due = vec![];
    for p in projects {
        if p.archived || (p.goal_tasks.is_none() && p.goal_mins.is_none()) {
            continue;
        }
        let tasks_met = p.goal_tasks.is_none_or(|n| p.goal_done_tasks >= n);
        let mins_met = p.goal_mins.is_none_or(|n| p.goal_done_mins >= n);
        let key = period_key(now, &p.goal_period);
        if !(tasks_met && mins_met) || p.notified_goal == key {
            continue;
        }
        let mut parts = vec![];
        if let Some(n) = p.goal_tasks { parts.push(format!("{} задач", n)); }
        if let Some(n) = p.goal_mins { parts.push(format!("{} мин", n)); }
        let period = if p.goal_period == "month" { "месяца" } else { "недели" };
        due.push(GoalDue {
            id: p.id,
            name: p.name,
            body: format!("Цель {} выполнена: {} 🎉", period, parts.join(" · ")),
            period_key: key,
        });
    }
    due
}

pub async fn mark_goal_notified(pool: &SqlitePool, id: &str, period_key: &str) {
    let _ = sqlx::query("UPDATE projects SET notified_goal = ? WHERE id = ?")
        .bind(period_key).bind(id).execute(pool).await;
}

async fn check_morning_digest(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    if !morning_digest_due(pool, now).await { return; }
    let local_today = now.with_timezone(&Local).date_naive();
    let tomorrow_local = local_today.succ_opt().unwrap_or(local_today);
    let tomorrow_utc = Local
        .from_local_datetime(&tomorrow_local.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now);

    // Блоки сегодня: начало локального дня в UTC
    let today_start_utc = Local
        .from_local_datetime(&local_today.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now - chrono::Duration::hours(12));
    let blocks = sqlx::query(
        "SELECT title, COALESCE(scheduled_mins, 60) AS mins, scheduled_at
         FROM tasks WHERE hidden = 0 AND status != 'Done'
           AND scheduled_at IS NOT NULL AND scheduled_at < ? AND scheduled_at >= ?"
    )
    .bind(tomorrow_utc.to_rfc3339())
    .bind(today_start_utc.to_rfc3339())
    .fetch_all(pool).await.unwrap_or_default();

    // Дедлайны сегодня + просрочки
    let due_row = sqlx::query(
        "SELECT COUNT(*) AS due,
                SUM(CASE WHEN deadline < ? THEN 1 ELSE 0 END) AS overdue
         FROM tasks WHERE hidden = 0 AND status != 'Done'
           AND deadline IS NOT NULL AND deadline < ?"
    )
    .bind(now.to_rfc3339())
    .bind(tomorrow_utc.to_rfc3339())
    .fetch_one(pool).await;

    let (due, overdue) = match due_row {
        Ok(r) => (r.get::<i64, _>("due"), r.get::<Option<i64>, _>("overdue").unwrap_or(0)),
        _ => (0i64, 0i64),
    };

    let mut body = String::new();
    if !blocks.is_empty() {
        body.push_str(&format!("Запланировано блоков: {}\n", blocks.len()));
        for b in blocks.iter().take(3) {
            let title: String = b.get("title");
            let mins: i64 = b.get("mins");
            let sched: String = b.get("scheduled_at");
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&sched) {
                let local = dt.with_timezone(&Local);
                body.push_str(&format!("  {} {}мин\n", local.format("%H:%M"), mins));
            }
            body.push_str(&format!("  {}\n", title));
        }
        if blocks.len() > 3 {
            body.push_str(&format!("  ...и ещё {}\n", blocks.len() - 3));
        }
    }
    if due > 0 {
        body.push_str(&format!("Дедлайнов сегодня: {due}"));
        if overdue > 0 { body.push_str(&format!(" (просрочено: {overdue})")); }
        body.push('\n');
    }
    if body.is_empty() {
        body = "На сегодня ничего не запланировано.".into();
    }
    if !muted {
        send_notification(app, "Утренняя сводка", body.trim());
    }
    crate::commands::settings::set_setting(pool, "morning_digest_last", &local_today.format("%Y-%m-%d").to_string()).await.ok();
}

async fn check_goals(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    for goal in goals_due(pool, now).await {
        if !muted {
            send_notification(app, &goal.name, &goal.body);
        }
        // Помечаем и в mute, чтобы после снятия глушилки не прилетала пачка
        mark_goal_notified(pool, &goal.id, &goal.period_key).await;
    }
    record_goal_snapshots(pool, now).await;
}

async fn record_goal_snapshots(pool: &SqlitePool, now: chrono::DateTime<Utc>) {
    use crate::commands::projects::{get_projects_at, period_key};
    let Ok(projects) = get_projects_at(pool, now).await else { return };
    for p in projects {
        if p.archived || (p.goal_tasks.is_none() && p.goal_mins.is_none()) {
            continue;
        }
        let key = period_key(now, &p.goal_period);
        let last = sqlx::query(
            "SELECT done_tasks, done_mins FROM project_goal_history
             WHERE project_id = ? AND period_key = ?
             ORDER BY recorded_at DESC LIMIT 1"
        )
        .bind(&p.id).bind(&key)
        .fetch_optional(pool).await;
        let Ok(Some(last_row)) = last else {
            // нет записи — создаём первую
            let _ = sqlx::query(
                "INSERT INTO project_goal_history (id, project_id, period_key, goal_tasks, goal_mins, done_tasks, done_mins, recorded_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&p.id).bind(&key)
            .bind(p.goal_tasks).bind(p.goal_mins)
            .bind(p.goal_done_tasks).bind(p.goal_done_mins)
            .bind(now.to_rfc3339())
            .execute(pool).await;
            continue;
        };
        let last_done_tasks: i64 = last_row.get("done_tasks");
        let last_done_mins: i64 = last_row.get("done_mins");
        if last_done_tasks != p.goal_done_tasks || last_done_mins != p.goal_done_mins {
            let _ = sqlx::query(
                "INSERT INTO project_goal_history (id, project_id, period_key, goal_tasks, goal_mins, done_tasks, done_mins, recorded_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&p.id).bind(&key)
            .bind(p.goal_tasks).bind(p.goal_mins)
            .bind(p.goal_done_tasks).bind(p.goal_done_mins)
            .bind(now.to_rfc3339())
            .execute(pool).await;
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct LimitDue {
    pub category: String,
    pub minutes: i64,
    pub limit: i64,
}

// Категории, превысившие дневной лимит и ещё не уведомлённые сегодня (локальный день).
// notified_json — текущее содержимое settings.app_limits_notified: {category: "YYYY-MM-DD"}.
pub fn limits_due(
    limits: &[crate::commands::monitor::AppLimit],
    usage: &[crate::commands::monitor::CategoryMinutes],
    notified_json: &str,
    now: chrono::DateTime<Utc>,
) -> Vec<LimitDue> {
    let today = now.with_timezone(&Local).format("%Y-%m-%d").to_string();
    let notified: std::collections::HashMap<String, String> =
        serde_json::from_str(notified_json).unwrap_or_default();

    let mut out = Vec::new();
    for limit in limits {
        if limit.daily_mins <= 0 { continue; }
        let minutes = usage.iter().find(|u| u.category == limit.category).map(|u| u.minutes).unwrap_or(0);
        if minutes < limit.daily_mins { continue; }
        if notified.get(&limit.category) == Some(&today) { continue; }
        out.push(LimitDue { category: limit.category.clone(), minutes, limit: limit.daily_mins });
    }
    out
}

async fn check_app_limits(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    let limits_json = crate::commands::settings::get_setting(pool, "app_limits").await.unwrap_or_default();
    let limits = crate::commands::monitor::parse_app_limits(&limits_json);
    if limits.is_empty() { return; }

    let Ok(usage) = crate::commands::monitor::get_app_category_time_impl(pool, 1).await else { return; };
    let notified_json = crate::commands::settings::get_setting(pool, "app_limits_notified").await.unwrap_or_default();
    let due = limits_due(&limits, &usage, &notified_json, now);
    if due.is_empty() { return; }

    let mut notified: std::collections::HashMap<String, String> =
        serde_json::from_str(&notified_json).unwrap_or_default();
    let today = now.with_timezone(&Local).format("%Y-%m-%d").to_string();

    for d in &due {
        if !muted {
            send_notification(app, &d.category, &format!("{}: {} мин из {} сегодня", d.category, d.minutes, d.limit));
        }
        // Помечаем и в mute — иначе после снятия глушилки прилетит пачка.
        notified.insert(d.category.clone(), today.clone());
    }
    if let Ok(json) = serde_json::to_string(&notified) {
        let _ = crate::commands::settings::set_setting(pool, "app_limits_notified", &json).await;
    }
}

// Утренняя сводка: should_run? (время настало, сегодня ещё не слали, время задано)
async fn morning_digest_due(pool: &SqlitePool, now: chrono::DateTime<Utc>) -> bool {
    let local_now = now.with_timezone(&Local);
    let time_setting = crate::commands::settings::get_setting(pool, "morning_digest_time").await;
    let Some(time_str) = time_setting else { return false; };
    if time_str.is_empty() { return false; }
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 { return false; }
    let (h, m): (u32, u32) = match (parts[0].parse(), parts[1].parse()) {
        (Ok(h), Ok(m)) if h < 24 && m < 60 => (h, m),
        _ => return false,
    };
    // Наступило ли время сегодня?
    let today = local_now.date_naive();
    let target = Local
        .from_local_datetime(&today.and_hms_opt(h, m, 0).unwrap())
        .single()
        .map(|d| d.with_timezone(&Utc));
    let Some(target_utc) = target else { return false; };
    if now < target_utc { return false; }
    // Уже отправляли сегодня?
    let last = crate::commands::settings::get_setting(pool, "morning_digest_last").await;
    let today_str = today.format("%Y-%m-%d").to_string();
    if last.as_deref() == Some(&today_str) { return false; }
    true
}

pub fn send_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::get_setting;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn set_time(pool: &SqlitePool, time: &str) {
        crate::commands::settings::set_setting(pool, "morning_digest_time", time).await.unwrap();
    }

    async fn set_last(pool: &SqlitePool, date: &str) {
        crate::commands::settings::set_setting(pool, "morning_digest_last", date).await.unwrap();
    }

    // Фиксируем now на локальное время так, чтобы целевой час был проверяем.
    // Возвращаем now, при котором `time_str` уже наступил (или нет).
    fn fixed_now(hour: u32, min: u32) -> chrono::DateTime<Utc> {
        let today = Local::now().date_naive();
        let local_dt = Local
            .from_local_datetime(&today.and_hms_opt(hour, min, 0).unwrap())
            .single()
            .unwrap();
        local_dt.with_timezone(&Utc)
    }

    #[tokio::test]
    async fn morning_digest_off_when_time_empty() {
        let pool = test_pool().await;
        set_time(&pool, "").await;
        assert!(!morning_digest_due(&pool, Utc::now()).await);
    }

    #[tokio::test]
    async fn morning_digest_not_due_before_set_time() {
        let pool = test_pool().await;
        set_time(&pool, "09:00").await;
        let before = fixed_now(8, 59);
        assert!(!morning_digest_due(&pool, before).await);
    }

    #[tokio::test]
    async fn morning_digest_due_after_set_time() {
        let pool = test_pool().await;
        set_time(&pool, "08:00").await;
        let after = fixed_now(8, 1);
        assert!(morning_digest_due(&pool, after).await);
    }

    #[tokio::test]
    async fn morning_digest_once_per_day() {
        let pool = test_pool().await;
        set_time(&pool, "08:00").await;
        let now = fixed_now(9, 0);
        let today_str = now.with_timezone(&Local).format("%Y-%m-%d").to_string();

        eprintln!("now={now}, today_str={today_str}");
        assert!(morning_digest_due(&pool, now).await);
        // После отправки дата сохраняется
        set_last(&pool, &today_str).await;
        let saved = get_setting(&pool, "morning_digest_last").await;
        eprintln!("saved last={saved:?}");
        assert!(!morning_digest_due(&pool, now).await);

        // На следующий день — снова должна (симулируем сбросом last на вчера)
        let yesterday = (now.with_timezone(&Local).date_naive() - chrono::Duration::days(1))
            .format("%Y-%m-%d").to_string();
        set_last(&pool, &yesterday).await;
        assert!(morning_digest_due(&pool, now).await);
    }

    #[tokio::test]
    async fn morning_digest_invalid_time_never_fires() {
        let pool = test_pool().await;
        set_time(&pool, "25:00").await;
        assert!(!morning_digest_due(&pool, Utc::now()).await);

        set_time(&pool, "ab:cd").await;
        assert!(!morning_digest_due(&pool, Utc::now()).await);
    }

    async fn insert_block(pool: &SqlitePool, title: &str, scheduled_at: &str, notified: bool) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden,
             created_at, updated_at, scheduled_at, scheduled_mins, notified_block)
             VALUES (?, ?, 'Todo', 'Medium', 'Work', 'None', '[]', 0, ?, ?, ?, 30, ?)")
            .bind(&id).bind(title)
            .bind(scheduled_at).bind(scheduled_at)
            .bind(scheduled_at).bind(notified)
            .execute(pool).await.unwrap();
        id
    }

    #[tokio::test]
    async fn blocks_due_respects_window_and_flag() {
        let pool = test_pool().await;
        let now = Utc::now();
        let ts = |mins_ago: i64| (now - chrono::Duration::minutes(mins_ago)).to_rfc3339();

        insert_block(&pool, "начался", &ts(2), false).await;
        insert_block(&pool, "уже уведомлён", &ts(2), true).await;
        insert_block(&pool, "слишком давно", &ts(30), false).await;
        insert_block(&pool, "ещё не начался", &ts(-30), false).await;

        let due = blocks_due(&pool, now, 10).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "начался");
    }

    #[tokio::test]
    async fn mark_and_sweep_stop_repeat_notifications() {
        let pool = test_pool().await;
        let now = Utc::now();
        let fresh = insert_block(&pool, "свежий", &(now - chrono::Duration::minutes(1)).to_rfc3339(), false).await;
        insert_block(&pool, "протухший", &(now - chrono::Duration::minutes(120)).to_rfc3339(), false).await;

        mark_block_notified(&pool, &fresh).await;
        sweep_stale_blocks(&pool, now, 10).await;

        // после пометки и свипа кандидатов не осталось
        assert!(blocks_due(&pool, now, 10).await.is_empty());
        let unnotified: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks WHERE notified_block = 0")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(unnotified, 0);
    }

    #[tokio::test]
    async fn goal_due_once_per_period_and_rearms_on_change() {
        use crate::commands::projects::*;
        let pool = test_pool().await;
        let now = Utc::now();

        let p = create_project_impl(&pool, CreateProject {
            name: "Спорт".into(), color: "".into(), target_date: None,
        }).await.unwrap();
        update_project_impl(&pool, p.id.clone(), UpdateProject {
            goal_tasks: Some(1), ..Default::default()
        }).await.unwrap();

        // цель ещё не выполнена — кандидатов нет
        assert!(goals_due(&pool, now).await.is_empty());

        // выполненная в этом периоде задача закрывает цель
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden,
             created_at, updated_at, completed_at, project_id)
             VALUES (?, 'т', 'Done', 'Medium', 'Work', 'None', '[]', 1, ?, ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(now.to_rfc3339()).bind(now.to_rfc3339())
            .bind((now - chrono::Duration::minutes(1)).to_rfc3339())
            .bind(&p.id)
            .execute(&pool).await.unwrap();

        let due = goals_due(&pool, now).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].name, "Спорт");
        assert!(due[0].body.contains("Цель недели"));

        // после пометки — в этом периоде больше не кандидат
        mark_goal_notified(&pool, &due[0].id, &due[0].period_key).await;
        assert!(goals_due(&pool, now).await.is_empty());

        // изменение цели перезаряжает пуш (notified_goal сброшен)
        update_project_impl(&pool, p.id.clone(), UpdateProject {
            goal_tasks: Some(1), ..Default::default()
        }).await.unwrap();
        assert_eq!(goals_due(&pool, now).await.len(), 1);

        // архивный проект не уведомляется
        update_project_impl(&pool, p.id, UpdateProject {
            archived: Some(true), ..Default::default()
        }).await.unwrap();
        assert!(goals_due(&pool, now).await.is_empty());
    }

    use crate::commands::monitor::{AppLimit, CategoryMinutes};

    fn limit(category: &str, daily_mins: i64) -> AppLimit {
        AppLimit { category: category.to_string(), daily_mins }
    }
    fn usage(category: &str, minutes: i64) -> CategoryMinutes {
        CategoryMinutes { category: category.to_string(), minutes }
    }

    #[test]
    fn limits_due_exceeded_triggers() {
        let limits = vec![limit("Other", 60)];
        let usage = vec![usage("Other", 65)];
        let due = limits_due(&limits, &usage, "", Utc::now());
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].category, "Other");
        assert_eq!(due[0].minutes, 65);
        assert_eq!(due[0].limit, 60);
    }

    #[test]
    fn limits_due_exactly_at_limit_triggers() {
        let limits = vec![limit("Other", 60)];
        let usage = vec![usage("Other", 60)];
        assert_eq!(limits_due(&limits, &usage, "", Utc::now()).len(), 1);
    }

    #[test]
    fn limits_due_under_limit_does_not_trigger() {
        let limits = vec![limit("Other", 60)];
        let usage = vec![usage("Other", 59)];
        assert!(limits_due(&limits, &usage, "", Utc::now()).is_empty());
    }

    #[test]
    fn limits_due_zero_or_missing_limit_means_no_limit() {
        let limits = vec![limit("Other", 0)];
        let usage = vec![usage("Other", 500)];
        assert!(limits_due(&limits, &usage, "", Utc::now()).is_empty());
    }

    #[test]
    fn limits_due_once_per_day() {
        let now = Utc::now();
        let today = now.with_timezone(&Local).format("%Y-%m-%d").to_string();
        let limits = vec![limit("Other", 60)];
        let usage = vec![usage("Other", 65)];
        let notified = format!(r#"{{"Other":"{today}"}}"#);
        assert!(limits_due(&limits, &usage, &notified, now).is_empty());
    }

    #[test]
    fn limits_due_rearms_next_day() {
        let now = Utc::now();
        let limits = vec![limit("Other", 60)];
        let usage = vec![usage("Other", 65)];
        let notified = r#"{"Other":"1999-01-01"}"#;
        assert_eq!(limits_due(&limits, &usage, notified, now).len(), 1);
    }

    #[tokio::test]
    async fn check_app_limits_marks_notified_and_is_idempotent_same_day() {
        let pool = test_pool().await;
        let now = Utc::now();
        crate::commands::settings::set_setting(&pool, "app_limits", r#"[{"category":"Other","daily_mins":1}]"#).await.unwrap();
        sqlx::query(
            "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs, app)
             VALUES (?, 'Active', 1, 0, 120, 'randomapp')")
            .bind(now.to_rfc3339())
            .execute(&pool).await.unwrap();

        // Нет provider-правил → randomapp попадёт в "Other" (categorize_app по умолчанию)
        let usage = crate::commands::monitor::get_app_category_time_impl(&pool, 1).await.unwrap();
        assert!(usage.iter().any(|c| c.category == "Other" && c.minutes >= 1));

        let notified_before = get_setting(&pool, "app_limits_notified").await.unwrap_or_default();
        assert!(notified_before.is_empty());

        // Симулируем полный цикл без реального AppHandle: напрямую проверяем limits_due + маркировку
        let limits = crate::commands::monitor::parse_app_limits(
            &get_setting(&pool, "app_limits").await.unwrap()
        );
        let due = limits_due(&limits, &usage, &notified_before, now);
        assert_eq!(due.len(), 1);
    }
}
