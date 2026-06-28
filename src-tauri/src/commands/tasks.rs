use tauri::State;
use sqlx::SqlitePool;
use chrono::{DateTime, Utc};
use crate::core::task::{CreateTask, Task, TaskRow, UpdateTask, TaskStatus};

#[tauri::command]
pub async fn create_task(
  pool: State<'_, SqlitePool>,
  task: CreateTask,
) -> Result<Task, String> {
  if task.title.trim().is_empty() {
    return Err("Название задачи не может быть пустым".into());
  }

  let new_task = task.into_task();

  sqlx::query(
    "INSERT INTO tasks (id, title, description, status, priority, category, deadline, tags, recurrence, hidden, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
  )
  .bind(&new_task.id)
  .bind(&new_task.title)
  .bind(&new_task.description)
  .bind(format!("{:?}", new_task.status))
  .bind(format!("{:?}", new_task.priority))
  .bind(format!("{:?}", new_task.category))
  .bind(new_task.deadline.map(|d| d.to_rfc3339()))
  .bind(serde_json::to_string(&new_task.tags).unwrap())
  .bind(new_task.recurrence.to_db()) 
  .bind(new_task.hidden)
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

#[tauri::command]
pub async fn delete_task(
  pool: State<'_, SqlitePool>,
  id: String,
) -> Result<(), String> {
  sqlx::query("DELETE FROM tasks WHERE id = ?")
    .bind(id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

  Ok(())
}

#[tauri::command]
pub async fn update_task(
    pool: State<'_, SqlitePool>,
    id: String,
    patch: UpdateTask,
) -> Result<Task, String> {
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    let mut task = row.into_task();
    let old_deadline = task.deadline;

    if let Some(title) = patch.title {
        if title.trim().is_empty() {
            return Err("Название задачи не может быть пустым".into());
        }
        task.title = title;
    }
    if let Some(desc) = patch.description   { task.description = Some(desc); }
    if let Some(status) = patch.status      { task.status = status; }
    if let Some(priority) = patch.priority  { task.priority = priority; }
    if let Some(category) = patch.category  { task.category = category; }
    if let Some(tags) = patch.tags          { task.tags = tags; }
    if let Some(recurrence) = patch.recurrence { task.recurrence = recurrence; }

    if let Some(dl) = patch.deadline {
        task.deadline = if dl.is_empty() {
            None
        } else {
            Some(DateTime::parse_from_rfc3339(&dl)
                .map_err(|e| e.to_string())?
                .with_timezone(&Utc))
        };
    }

    task.updated_at = Utc::now();
    // Если дедлайн реально изменился, старые флаги notified_* больше не
    // отражают актуальный дедлайн — иначе планировщик никогда не пришлёт
    // уведомление по новой дате (раньше это был баг: флаги не сбрасывались).
    let deadline_changed = task.deadline != old_deadline;

    sqlx::query(
        "UPDATE tasks SET title=?, description=?, status=?, priority=?,
         category=?, deadline=?, tags=?, recurrence=?, updated_at=?,
         notified_24h = CASE WHEN ? THEN 0 ELSE notified_24h END,
         notified_1h = CASE WHEN ? THEN 0 ELSE notified_1h END,
         notified_deadline = CASE WHEN ? THEN 0 ELSE notified_deadline END
         WHERE id=?"
    )
    .bind(&task.title)
    .bind(&task.description)
    .bind(format!("{:?}", task.status))
    .bind(format!("{:?}", task.priority))
    .bind(format!("{:?}", task.category))
    .bind(task.deadline.map(|d| d.to_rfc3339()))
    .bind(serde_json::to_string(&task.tags).unwrap())
    .bind(task.recurrence.to_db())
    .bind(task.updated_at.to_rfc3339())
    .bind(deadline_changed)
    .bind(deadline_changed)
    .bind(deadline_changed)
    .bind(&id)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(task)
}

#[tauri::command]
pub async fn complete_task(
  pool: State<'_, SqlitePool>,
  id: String,
) -> Result<Task, String> {
  let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

  let mut task = row.into_task();
  let now = Utc::now();
  // recurring задачи едут на новый дедлайн -> старые notified_* флаги
  // относятся к ПРОШЛОМУ дедлайну. Если их не сбросить, scheduler никогда
  // больше не уведомит об этой задаче (баг: флаги раньше не сбрасывались).
  let mut reset_notifications = false;

  match task.recurrence.to_duration() {
    None => {
      task.status = TaskStatus::Done;
      task.hidden = true;
      task.completed_at = Some(now);
    }
    Some(duration) => {
      task.deadline = Some(now + duration);
      reset_notifications = true;
    }
  }

  task.updated_at = now;

  sqlx::query(
    "UPDATE tasks SET status=?, hidden=?, deadline=?, completed_at=?, updated_at=?,
     notified_24h = CASE WHEN ? THEN 0 ELSE notified_24h END,
     notified_1h = CASE WHEN ? THEN 0 ELSE notified_1h END,
     notified_deadline = CASE WHEN ? THEN 0 ELSE notified_deadline END
     WHERE id=?"
  )
  .bind(format!("{:?}", task.status))
  .bind(task.hidden)
  .bind(task.deadline.map(|d| d.to_rfc3339()))
  .bind(task.completed_at.map(|d| d.to_rfc3339()))
  .bind(task.updated_at.to_rfc3339())
  .bind(reset_notifications)
  .bind(reset_notifications)
  .bind(reset_notifications)
  .bind(&id)
  .execute(pool.inner())
  .await
  .map_err(|e| e.to_string())?;

  Ok(task)
}

#[tauri::command]
pub async fn search_tasks(
  pool: State<'_, SqlitePool>,
  query: String,
) -> Result<Vec<Task>, String> {
  let trimmed = query.trim();
  if trimmed.is_empty() {
    return Ok(vec![]);
  }

  // Сырой ввод пользователя нельзя пускать в MATCH напрямую: символы вроде
  // " - : ( ) AND/OR/NOT — это синтаксис FTS5, а не текст. Дефис в названии
  // ("купить хлеб-2") уже падал с "no such column: 2". Оборачиваем как
  // quoted-phrase-prefix: безопасно для любого ввода.
  let escaped = trimmed.replace('"', "\"\"");
  let fts_query = format!("\"{}\"*", escaped);

  let rows = sqlx::query_as::<_, TaskRow>(
    "SELECT t.* FROM tasks t
     INNER JOIN tasks_fts ON tasks_fts.rowid = t.rowid
     WHERE tasks_fts MATCH ?
       AND t.hidden = 0
     ORDER BY rank"
  )
  .bind(fts_query)
  .fetch_all(pool.inner())
  .await
  .map_err(|e| e.to_string())?;

  Ok(rows.into_iter().map(|r| r.into_task()).collect())
}
