use chrono::Utc;
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

async fn check_goals(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    for goal in goals_due(pool, now).await {
        if !muted {
            send_notification(app, &goal.name, &goal.body);
        }
        // Помечаем и в mute, чтобы после снятия глушилки не прилетала пачка
        mark_goal_notified(pool, &goal.id, &goal.period_key).await;
    }
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

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
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
}
