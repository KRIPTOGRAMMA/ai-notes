use chrono::Utc;
use std::sync::{Arc, Mutex};
use sqlx::{SqlitePool, Row};
use tauri_plugin_notification::NotificationExt;
use crate::commands::settings::WorkMode;

pub fn start_scheduler(app: tauri::AppHandle, pool: SqlitePool, work_mode: Arc<Mutex<WorkMode>>) {
    tokio::spawn(async move {
        loop {
            // В режиме Focus дедлайны всё равно проверяем и помечаем, но не шлём:
            // иначе после выхода из Focus прилетит пачка устаревших уведомлений.
            let muted = *work_mode.lock().unwrap() == WorkMode::Focus;
            check_deadlines(&app, &pool, muted).await;
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });
}

async fn check_deadlines(app: &tauri::AppHandle, pool: &SqlitePool, muted: bool) {
    let now = Utc::now();
    let in_1h = now + chrono::Duration::hours(1);
    let in_24h = now + chrono::Duration::hours(24);

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

        if !notified_24h && deadline <= in_24h && deadline > in_1h {
            if !muted { send_notification(app, &title, "Дедлайн через 24 часа"); }
            let _ = sqlx::query("UPDATE tasks SET notified_24h = 1 WHERE id = ?")
                .bind(&id).execute(pool).await;
        }

        if !notified_1h && deadline <= in_1h && deadline > now {
            if !muted { send_notification(app, &title, "Дедлайн через 1 час"); }
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
