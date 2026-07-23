use tauri::State;
use sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::AppResult;

// Статус-фолбэк для невалидных/удалённых значений — тот же принцип, что
// FALLBACK_CATEGORY у categories.rs.
pub const FALLBACK_STATUS: &str = "Todo";

// Исходные 4 статуса (бывшие варианты enum TaskStatus) — с ними завязана
// бизнес-логика (Done → hidden+completed_at в complete_task, InProgress →
// тайм-трекинг, дедлайн/триггер-запросы сравнивают напрямую со строками),
// поэтому не переименовываются и не удаляются, в отличие от пользовательских
// колонок канбана.
pub const RESERVED_STATUSES: [&str; 4] = ["Todo", "InProgress", "Done", "Archived"];

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, sqlx::FromRow)]
pub struct Status {
    pub id: String,
    pub name: String,
    pub color: String,
    pub position: i64,
    pub is_reserved: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateStatus {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[tauri::command]
pub async fn get_statuses(pool: State<'_, SqlitePool>) -> AppResult<Vec<Status>> {
    get_statuses_impl(pool.inner()).await
}

pub async fn get_statuses_impl(pool: &SqlitePool) -> AppResult<Vec<Status>> {
    Ok(sqlx::query_as::<_, Status>(
        "SELECT id, name, color, position, is_reserved FROM statuses ORDER BY position, name",
    )
    .fetch_all(pool)
    .await?)
}

#[tauri::command]
pub async fn create_status(pool: State<'_, SqlitePool>, name: String, color: String) -> AppResult<Status> {
    create_status_impl(pool.inner(), name, color).await
}

pub async fn create_status_impl(pool: &SqlitePool, name: String, color: String) -> AppResult<Status> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Название статуса не может быть пустым".to_string().into());
    }
    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM statuses")
        .fetch_one(pool)
        .await?;
    let status = Status {
        id: Uuid::new_v4().to_string(),
        name,
        color: if color.trim().is_empty() { "#888888".into() } else { color },
        position,
        is_reserved: false,
    };
    sqlx::query("INSERT INTO statuses (id, name, color, position, is_reserved) VALUES (?, ?, ?, ?, 0)")
        .bind(&status.id)
        .bind(&status.name)
        .bind(&status.color)
        .bind(status.position)
        .execute(pool)
        .await?;
    Ok(status)
}

#[tauri::command]
pub async fn update_status(pool: State<'_, SqlitePool>, id: String, patch: UpdateStatus) -> AppResult<()> {
    update_status_impl(pool.inner(), id, patch).await
}

pub async fn update_status_impl(pool: &SqlitePool, id: String, patch: UpdateStatus) -> AppResult<()> {
    if RESERVED_STATUSES.contains(&id.as_str()) && patch.name.is_some() {
        return Err("Встроенный статус нельзя переименовать".to_string().into());
    }
    if let Some(name) = patch.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Название статуса не может быть пустым".to_string().into());
        }
        sqlx::query("UPDATE statuses SET name = ? WHERE id = ?")
            .bind(&name).bind(&id)
            .execute(pool).await?;
    }
    if let Some(color) = patch.color {
        sqlx::query("UPDATE statuses SET color = ? WHERE id = ?")
            .bind(&color).bind(&id)
            .execute(pool).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_status(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_status_impl(pool.inner(), id).await
}

pub async fn delete_status_impl(pool: &SqlitePool, id: String) -> AppResult<()> {
    if RESERVED_STATUSES.contains(&id.as_str()) {
        return Err("Встроенный статус нельзя удалить".to_string().into());
    }
    // Задачи удаляемого статуса переезжают в фолбэк (Todo)
    sqlx::query("UPDATE tasks SET status = ? WHERE status = ?")
        .bind(FALLBACK_STATUS)
        .bind(&id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM statuses WHERE id = ?")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(())
}

// Валидация статуса на записи задачи: неизвестный id тихо становится
// фолбэком (Todo) — тот же принцип, что valid_or_fallback у категорий.
pub async fn valid_or_fallback(pool: &SqlitePool, status: &str) -> String {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM statuses WHERE id = ?")
        .bind(status)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    if exists.is_some() {
        status.to_string()
    } else {
        FALLBACK_STATUS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn seeded_statuses_present_and_ordered() {
        let pool = test_pool().await;
        let statuses = get_statuses_impl(&pool).await.unwrap();
        let ids: Vec<&str> = statuses.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["Todo", "InProgress", "Done", "Archived"]);
        assert!(statuses.iter().all(|s| s.is_reserved));
    }

    #[tokio::test]
    async fn crud_roundtrip_and_position() {
        let pool = test_pool().await;

        let status = create_status_impl(&pool, "На ревью".into(), "#ff0000".into()).await.unwrap();
        assert_eq!(status.position, 4); // после посевных 0..3
        assert!(!status.is_reserved);

        update_status_impl(&pool, status.id.clone(), UpdateStatus {
            name: Some("Ревью".into()),
            color: Some("#00ff00".into()),
        }).await.unwrap();
        let statuses = get_statuses_impl(&pool).await.unwrap();
        let found = statuses.iter().find(|s| s.id == status.id).unwrap();
        assert_eq!(found.name, "Ревью");
        assert_eq!(found.color, "#00ff00");

        assert!(create_status_impl(&pool, "   ".into(), "".into()).await.is_err());

        delete_status_impl(&pool, status.id.clone()).await.unwrap();
        assert!(get_statuses_impl(&pool).await.unwrap().iter().all(|s| s.id != status.id));
    }

    #[tokio::test]
    async fn delete_reassigns_tasks_to_todo() {
        let pool = test_pool().await;
        let status = create_status_impl(&pool, "Временный".into(), "#123456".into()).await.unwrap();

        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden, created_at, updated_at)
             VALUES ('t1', 'задача', ?, 'Medium', 'Other', 'None', '[]', 0, '2026-07-16T10:00:00+00:00', '2026-07-16T10:00:00+00:00')",
        )
        .bind(&status.id)
        .execute(&pool).await.unwrap();

        delete_status_impl(&pool, status.id).await.unwrap();
        let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(task_status, FALLBACK_STATUS);
    }

    #[tokio::test]
    async fn reserved_statuses_cannot_be_deleted_or_renamed() {
        let pool = test_pool().await;
        for id in RESERVED_STATUSES {
            assert!(delete_status_impl(&pool, id.into()).await.is_err());
            assert!(update_status_impl(&pool, id.into(), UpdateStatus {
                name: Some("Хак".into()), color: None,
            }).await.is_err());
        }
    }

    #[tokio::test]
    async fn reserved_status_color_can_still_be_customized() {
        let pool = test_pool().await;
        // Только имя защищено — цвет менять можно (косметика, не логика)
        update_status_impl(&pool, "Done".into(), UpdateStatus {
            name: None, color: Some("#123123".into()),
        }).await.unwrap();
        let statuses = get_statuses_impl(&pool).await.unwrap();
        assert_eq!(statuses.iter().find(|s| s.id == "Done").unwrap().color, "#123123");
    }

    #[tokio::test]
    async fn valid_or_fallback_checks_table() {
        let pool = test_pool().await;
        assert_eq!(valid_or_fallback(&pool, "Done").await, "Done");
        assert_eq!(valid_or_fallback(&pool, "???").await, "Todo");
        let status = create_status_impl(&pool, "Новый".into(), "".into()).await.unwrap();
        assert_eq!(valid_or_fallback(&pool, &status.id).await, status.id);
    }
}
