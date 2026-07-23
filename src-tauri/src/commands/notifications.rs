use tauri::State;
use sqlx::{Row, SqlitePool};
use serde::Serialize;
use crate::error::AppResult;

// Центр уведомлений (v0.9.16): лента из notification_log — история пушей
// внутри приложения, populate'ится централизованно в notifier::scheduler::send_notification.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct NotificationEntry {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub read_at: Option<String>,
    // v0.9.18: если задано — клик по записи в Центре уведомлений открывает
    // эту сущность (сейчас только "note", задел под другие типы на будущее).
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
}

fn row_to_entry(row: sqlx::sqlite::SqliteRow) -> NotificationEntry {
    NotificationEntry {
        id: row.get("id"),
        kind: row.get("kind"),
        title: row.get("title"),
        body: row.get("body"),
        created_at: row.get("created_at"),
        read_at: row.get("read_at"),
        entity_type: row.get("entity_type"),
        entity_id: row.get("entity_id"),
    }
}

const FEED_LIMIT: i64 = 100;

#[tauri::command]
pub async fn get_notification_log(pool: State<'_, SqlitePool>) -> AppResult<Vec<NotificationEntry>> {
    get_notification_log_impl(pool.inner()).await
}

pub async fn get_notification_log_impl(pool: &SqlitePool) -> AppResult<Vec<NotificationEntry>> {
    let rows = sqlx::query(
        "SELECT id, kind, title, body, created_at, read_at, entity_type, entity_id FROM notification_log
         ORDER BY created_at DESC LIMIT ?"
    )
    .bind(FEED_LIMIT)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_entry).collect())
}

#[tauri::command]
pub async fn get_unread_notification_count(pool: State<'_, SqlitePool>) -> AppResult<i64> {
    get_unread_notification_count_impl(pool.inner()).await
}

pub async fn get_unread_notification_count_impl(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM notification_log WHERE read_at IS NULL")
        .fetch_one(pool)
        .await?)
}

#[tauri::command]
pub async fn mark_notifications_read(pool: State<'_, SqlitePool>) -> AppResult<()> {
    mark_notifications_read_impl(pool.inner()).await
}

pub async fn mark_notifications_read_impl(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("UPDATE notification_log SET read_at = ? WHERE read_at IS NULL")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn clear_notification_log(pool: State<'_, SqlitePool>) -> AppResult<()> {
    clear_notification_log_impl(pool.inner()).await
}

pub async fn clear_notification_log_impl(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("DELETE FROM notification_log").execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert(pool: &SqlitePool, kind: &str, title: &str, created_at: chrono::DateTime<chrono::Utc>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO notification_log (id, kind, title, body, created_at) VALUES (?, ?, ?, 'body', ?)")
            .bind(&id).bind(kind).bind(title).bind(created_at.to_rfc3339())
            .execute(pool).await.unwrap();
        id
    }

    #[tokio::test]
    async fn feed_orders_newest_first() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        insert(&pool, "deadline", "старое", now - chrono::Duration::hours(2)).await;
        insert(&pool, "block", "новое", now).await;

        let feed = get_notification_log_impl(&pool).await.unwrap();
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].title, "новое");
        assert_eq!(feed[1].title, "старое");
    }

    // v0.9.18: записи без entity-ссылки (большинство существующих kind) —
    // entity_type/entity_id должны читаться как None, не падать/паниковать.
    #[tokio::test]
    async fn entries_without_entity_ref_read_as_none() {
        let pool = test_pool().await;
        insert(&pool, "deadline", "a", chrono::Utc::now()).await;
        let feed = get_notification_log_impl(&pool).await.unwrap();
        assert_eq!(feed[0].entity_type, None);
        assert_eq!(feed[0].entity_id, None);
    }

    #[tokio::test]
    async fn entry_with_entity_ref_roundtrips() {
        let pool = test_pool().await;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO notification_log (id, kind, title, body, created_at, entity_type, entity_id)
             VALUES (?, 'note_reminder', 'заметка', 'body', ?, 'note', 'note-42')"
        )
        .bind(&id).bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool).await.unwrap();

        let feed = get_notification_log_impl(&pool).await.unwrap();
        assert_eq!(feed[0].entity_type.as_deref(), Some("note"));
        assert_eq!(feed[0].entity_id.as_deref(), Some("note-42"));
    }

    #[tokio::test]
    async fn unread_count_and_mark_read() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        insert(&pool, "deadline", "a", now).await;
        insert(&pool, "block", "b", now).await;

        assert_eq!(get_unread_notification_count_impl(&pool).await.unwrap(), 2);

        mark_notifications_read_impl(&pool).await.unwrap();
        assert_eq!(get_unread_notification_count_impl(&pool).await.unwrap(), 0);

        let feed = get_notification_log_impl(&pool).await.unwrap();
        assert!(feed.iter().all(|e| e.read_at.is_some()));
    }

    #[tokio::test]
    async fn new_notification_after_mark_read_is_unread_again() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        insert(&pool, "deadline", "a", now).await;
        mark_notifications_read_impl(&pool).await.unwrap();
        assert_eq!(get_unread_notification_count_impl(&pool).await.unwrap(), 0);

        insert(&pool, "block", "b", now).await;
        assert_eq!(get_unread_notification_count_impl(&pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn clear_removes_all() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        insert(&pool, "deadline", "a", now).await;
        insert(&pool, "block", "b", now).await;

        clear_notification_log_impl(&pool).await.unwrap();
        assert!(get_notification_log_impl(&pool).await.unwrap().is_empty());
        assert_eq!(get_unread_notification_count_impl(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn feed_respects_limit() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        for i in 0..105 {
            insert(&pool, "deadline", &format!("n{i}"), now - chrono::Duration::minutes(i)).await;
        }
        let feed = get_notification_log_impl(&pool).await.unwrap();
        assert_eq!(feed.len(), 100);
        assert_eq!(feed[0].title, "n0"); // самое свежее (наименьший offset) первым
    }
}
