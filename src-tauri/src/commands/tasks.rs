use tauri::State;
use sqlx::SqlitePool;
use crate::core::task::{CreateTask, Task, TaskRow};

#[tauri::command]
pub async fn create_task(
  pool: State<'_, SqlitePool>,
  task: CreateTask,
) -> Result<Task, String> {
  let new_task = task.into_task();

  sqlx::query(
    "INSERT INTO tasks (id, title, description, status, priority, category, deadline, tags, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
  )
  .bind(&new_task.id)
  .bind(&new_task.title)
  .bind(&new_task.description)
  .bind(format!("{:?}", new_task.status))
  .bind(format!("{:?}", new_task.priority))
  .bind(format!("{:?}", new_task.category))
  .bind(new_task.deadline.map(|d| d.to_rfc3339()))
  .bind(serde_json::to_string(&new_task.tags).unwrap())
  .bind(new_task.created_at.to_rfc3339())
  .bind(new_task.updated_at.to_rfc3339())
  .execute(pool.inner())
  .await
  .map_err(|e| e.to_string())?;

  Ok(new_task)
}

#[tauri::command]
pub async fn get_tasks(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<Task>, String> {
    let rows = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks")
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(|r| r.into_task()).collect())
}