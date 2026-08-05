use tauri::State;
use sqlx::SqlitePool;
use uuid::Uuid;
use chrono::Utc;
use crate::core::task::{Subtask, Task};
use crate::error::{AppError, AppResult};

// Fills subtasks into already-loaded tasks with a single query.
pub async fn attach_subtasks(pool: &SqlitePool, tasks: &mut [Task]) -> AppResult<()> {
    if tasks.is_empty() {
        return Ok(());
    }
    let all = sqlx::query_as::<_, Subtask>(
        "SELECT id, task_id, title, done, position FROM subtasks ORDER BY position, created_at"
    )
    .fetch_all(pool)
    .await?;

    for task in tasks.iter_mut() {
        task.subtasks = all.iter().filter(|s| s.task_id == task.id).cloned().collect();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_subtasks(pool: State<'_, SqlitePool>, task_id: String) -> AppResult<Vec<Subtask>> {
    get_subtasks_impl(pool.inner(), &task_id).await
}

pub async fn get_subtasks_impl(pool: &SqlitePool, task_id: &str) -> AppResult<Vec<Subtask>> {
    sqlx::query_as::<_, Subtask>(
        "SELECT id, task_id, title, done, position FROM subtasks
         WHERE task_id = ? ORDER BY position, created_at"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn add_subtask(pool: State<'_, SqlitePool>, task_id: String, title: String) -> AppResult<Subtask> {
    add_subtask_impl(pool.inner(), &task_id, &title).await
}

pub async fn add_subtask_impl(pool: &SqlitePool, task_id: &str, title: &str) -> AppResult<Subtask> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::Other("Пустая подзадача".into()));
    }
    let id = Uuid::new_v4().to_string();
    // position = the end of the list
    let next_pos: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position) + 1, 0) FROM subtasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await?;

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
    .await?;

    Ok(Subtask { id, task_id: task_id.to_string(), title: title.to_string(), done: false, position: next_pos })
}

#[tauri::command]
pub async fn toggle_subtask(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    toggle_subtask_impl(pool.inner(), &id).await
}

pub async fn toggle_subtask_impl(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("UPDATE subtasks SET done = 1 - done WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Inline title editing in the modal's checklist: an empty title is an error
// rather than a silent deletion — deleting is an explicit operation.
#[tauri::command]
pub async fn rename_subtask(pool: State<'_, SqlitePool>, id: String, title: String) -> AppResult<()> {
    rename_subtask_impl(pool.inner(), &id, &title).await
}

pub async fn rename_subtask_impl(pool: &SqlitePool, id: &str, title: &str) -> AppResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::Other("Пустая подзадача".into()));
    }
    sqlx::query("UPDATE subtasks SET title = ? WHERE id = ?")
        .bind(title)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_subtask(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_subtask_impl(pool.inner(), &id).await
}

pub async fn delete_subtask_impl(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM subtasks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
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
    async fn rename_updates_title_and_rejects_empty() {
        let pool = test_pool().await;
        let s = add_subtask_impl(&pool, "task-1", "старое").await.unwrap();

        rename_subtask_impl(&pool, &s.id, "  новое  ").await.unwrap();
        let list = get_subtasks_impl(&pool, "task-1").await.unwrap();
        assert_eq!(list[0].title, "новое"); // trim

        assert!(rename_subtask_impl(&pool, &s.id, "   ").await.is_err());
        // the title was not clobbered by the rejected rename
        assert_eq!(get_subtasks_impl(&pool, "task-1").await.unwrap()[0].title, "новое");
    }

    #[tokio::test]
    async fn position_increments_per_task() {
        let pool = test_pool().await;
        let a = add_subtask_impl(&pool, "t", "1").await.unwrap();
        let b = add_subtask_impl(&pool, "t", "2").await.unwrap();
        assert_eq!(a.position, 0);
        assert_eq!(b.position, 1);
    }

    // The point of the AppError migration (v0.9.83), asserted rather than assumed.
    //
    // These commands used to return Result<_, String> built by
    // `.map_err(|e| e.to_string())`, so a database failure reached the frontend
    // as bare sqlx text with no prefix. errorText.ts translates by matching a
    // closed set of prefixes and returns anything else untouched, so those
    // messages silently skipped localization — add_subtask is the most frequent
    // command that was affected.
    //
    // Dropping the table is the cheapest real sqlx failure; the assertion is
    // about the prefix, not about that particular SQL error.
    #[tokio::test]
    async fn db_failure_carries_the_translatable_prefix() {
        let pool = test_pool().await;
        sqlx::query("DROP TABLE subtasks").execute(&pool).await.unwrap();

        let err = add_subtask_impl(&pool, "task-1", "заголовок").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.starts_with("Ошибка базы данных: "),
            "ошибка БД пришла без префикса, errorText.ts её не переведёт: {msg}"
        );

        // The same for a read, which returns the error as a tail expression
        // (map_err(AppError::from)) rather than through `?` — a different path
        // through the same conversion.
        let err = get_subtasks_impl(&pool, "task-1").await.unwrap_err();
        assert!(
            err.to_string().starts_with("Ошибка базы данных: "),
            "чтение подзадач вернуло ошибку без префикса: {err}"
        );
    }

    // Domain messages must stay verbatim: errorText.ts splits on a known prefix,
    // and "Пустая подзадача" is not one — gluing a technical prefix onto it would
    // both mistranslate it and change what the user reads.
    #[tokio::test]
    async fn domain_error_stays_verbatim() {
        let pool = test_pool().await;
        let err = add_subtask_impl(&pool, "task-1", "   ").await.unwrap_err();
        assert_eq!(err.to_string(), "Пустая подзадача");
    }
}
