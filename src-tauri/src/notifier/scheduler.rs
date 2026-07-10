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

pub fn send_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}
