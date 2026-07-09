use tauri::State;
use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;
use crate::core::task::{Subtask, Task};

// Проставляет подзадачи в уже загруженные задачи одним запросом.
pub async fn attach_subtasks(pool: &SqlitePool, tasks: &mut [Task]) -> Result<(), String> {
    if tasks.is_empty() {
        return Ok(());
    }
    let all = sqlx::query_as::<_, Subtask>(
        "SELECT id, task_id, title, done, position FROM subtasks ORDER BY position, created_at"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for task in tasks.iter_mut() {
        task.subtasks = all.iter().filter(|s| s.task_id == task.id).cloned().collect();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_subtasks(pool: State<'_, SqlitePool>, task_id: String) -> Result<Vec<Subtask>, String> {
    get_subtasks_impl(pool.inner(), &task_id).await
}

pub async fn get_subtasks_impl(pool: &SqlitePool, task_id: &str) -> Result<Vec<Subtask>, String> {
    sqlx::query_as::<_, Subtask>(
        "SELECT id, task_id, title, done, position FROM subtasks
         WHERE task_id = ? ORDER BY position, created_at"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_subtask(pool: State<'_, SqlitePool>, task_id: String, title: String) -> Result<Subtask, String> {
    add_subtask_impl(pool.inner(), &task_id, &title).await
}

pub async fn add_subtask_impl(pool: &SqlitePool, task_id: &str, title: &str) -> Result<Subtask, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Пустая подзадача".into());
    }
    let id = Uuid::new_v4().to_string();
    // position = в конец списка
    let next_pos: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position) + 1, 0) FROM subtasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO subtasks (id, task_id, title, done, position, created_at)
         VALUES (?, ?, ?, 0, ?, ?)"
    )
    .bind(&id)
    .bind(task_id)
    .bind(title)
    .bind(next_pos)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(Subtask { id, task_id: task_id.to_string(), title: title.to_string(), done: false, position: next_pos })
}

#[tauri::command]
pub async fn toggle_subtask(pool: State<'_, SqlitePool>, id: String) -> Result<(), String> {
    toggle_subtask_impl(pool.inner(), &id).await
}

pub async fn toggle_subtask_impl(pool: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("UPDATE subtasks SET done = 1 - done WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_subtask(pool: State<'_, SqlitePool>, id: String) -> Result<(), String> {
    delete_subtask_impl(pool.inner(), &id).await
}

pub async fn delete_subtask_impl(pool: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM subtasks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
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

    #[tokio::test]
    async fn add_toggle_delete_roundtrip() {
        let pool = test_pool().await;
        let s = add_subtask_impl(&pool, "task-1", "  купить хлеб  ").await.unwrap();
        assert_eq!(s.title, "купить хлеб"); // trim
        assert!(!s.done);

        let list = get_subtasks_impl(&pool, "task-1").await.unwrap();
        assert_eq!(list.len(), 1);

        toggle_subtask_impl(&pool, &s.id).await.unwrap();
        assert!(get_subtasks_impl(&pool, "task-1").await.unwrap()[0].done);
        toggle_subtask_impl(&pool, &s.id).await.unwrap();
        assert!(!get_subtasks_impl(&pool, "task-1").await.unwrap()[0].done);

        delete_subtask_impl(&pool, &s.id).await.unwrap();
        assert!(get_subtasks_impl(&pool, "task-1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_title_rejected() {
        let pool = test_pool().await;
        assert!(add_subtask_impl(&pool, "task-1", "   ").await.is_err());
    }

    #[tokio::test]
    async fn position_increments_per_task() {
        let pool = test_pool().await;
        let a = add_subtask_impl(&pool, "t", "1").await.unwrap();
        let b = add_subtask_impl(&pool, "t", "2").await.unwrap();
        assert_eq!(a.position, 0);
        assert_eq!(b.position, 1);
    }
}
