use tauri::State;
use sqlx::{SqlitePool, Row};
use serde::Serialize;
use chrono::{DateTime, Utc};
use crate::core::task::{CreateTask, Task, TaskRow, UpdateTask, TaskStatus};

#[tauri::command]
pub async fn create_task(
  pool: State<'_, SqlitePool>,
  task: CreateTask,
) -> Result<Task, String> {
  create_task_impl(pool.inner(), task).await
}

pub async fn create_task_impl(pool: &SqlitePool, task: CreateTask) -> Result<Task, String> {
  if task.title.trim().is_empty() {
    return Err("Название задачи не может быть пустым".into());
  }

  let mut new_task = task.into_task();
  // Неизвестная категория тихо становится фолбэком (прежняя семантика enum)
  new_task.category = crate::commands::categories::valid_or_fallback(pool, &new_task.category).await;
  // Новая задача — в конец списка
  new_task.sort_order = sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks")
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

  sqlx::query(
    "INSERT INTO tasks (id, title, description, status, priority, category, deadline, tags, recurrence, hidden, created_at, updated_at, project_id, sort_order)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
  )
  .bind(&new_task.id)
  .bind(&new_task.title)
  .bind(&new_task.description)
  .bind(format!("{:?}", new_task.status))
  .bind(format!("{:?}", new_task.priority))
  .bind(&new_task.category)
  .bind(new_task.deadline.map(|d| d.to_rfc3339()))
  .bind(serde_json::to_string(&new_task.tags).unwrap_or_else(|_| "[]".into()))
  .bind(new_task.recurrence.to_db()) 
  .bind(new_task.hidden)
  .bind(new_task.created_at.to_rfc3339())
  .bind(new_task.updated_at.to_rfc3339())
  .bind(&new_task.project_id)
  .bind(new_task.sort_order)
  .execute(pool)
  .await
  .map_err(|e| e.to_string())?;

  Ok(new_task)
}

#[tauri::command]
pub async fn get_tasks(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<Task>, String> {
    get_tasks_impl(pool.inner()).await
}

pub async fn get_tasks_impl(pool: &SqlitePool) -> Result<Vec<Task>, String> {
    let rows = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE deleted_at IS NULL ORDER BY sort_order")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut tasks: Vec<Task> = rows.into_iter().map(|r| r.into_task()).collect();
    crate::commands::subtasks::attach_subtasks(pool, &mut tasks).await?;
    Ok(tasks)
}

#[tauri::command]
pub async fn delete_task(
  pool: State<'_, SqlitePool>,
  id: String,
) -> Result<(), String> {
  delete_task_impl(pool.inner(), id).await
}

// Мягкое удаление (v0.8.12, «Корзина»): задача остаётся в таблице (со
// своими подзадачами и привязками заметок нетронутыми), просто перестаёт
// показываться в активных/истории — фильтруется в get_tasks_impl.
// Настоящее удаление — через purge_deleted_task.
pub async fn delete_task_impl(pool: &SqlitePool, id: String) -> Result<(), String> {
  sqlx::query("UPDATE tasks SET deleted_at = ? WHERE id = ?")
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

  Ok(())
}

#[tauri::command]
pub async fn get_deleted_tasks(pool: State<'_, SqlitePool>) -> Result<Vec<Task>, String> {
  get_deleted_tasks_impl(pool.inner()).await
}

pub async fn get_deleted_tasks_impl(pool: &SqlitePool) -> Result<Vec<Task>, String> {
  let rows = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC")
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

  let mut tasks: Vec<Task> = rows.into_iter().map(|r| r.into_task()).collect();
  crate::commands::subtasks::attach_subtasks(pool, &mut tasks).await?;
  Ok(tasks)
}

#[tauri::command]
pub async fn restore_task(pool: State<'_, SqlitePool>, id: String) -> Result<(), String> {
  restore_task_impl(pool.inner(), id).await
}

pub async fn restore_task_impl(pool: &SqlitePool, id: String) -> Result<(), String> {
  sqlx::query("UPDATE tasks SET deleted_at = NULL WHERE id = ?")
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

  Ok(())
}

#[tauri::command]
pub async fn purge_deleted_task(pool: State<'_, SqlitePool>, id: String) -> Result<(), String> {
  purge_deleted_task_impl(pool.inner(), id).await
}

// Настоящее удаление строки из корзины — та же зачистка подзадач/привязок
// заметок, что раньше делал delete_task_impl (жёсткое удаление).
pub async fn purge_deleted_task_impl(pool: &SqlitePool, id: String) -> Result<(), String> {
  sqlx::query("DELETE FROM subtasks WHERE task_id = ?")
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

  sqlx::query("UPDATE notes SET linked_task_id = NULL WHERE linked_task_id = ?")
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

  sqlx::query("DELETE FROM tasks WHERE id = ?")
    .bind(id)
    .execute(pool)
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
    update_task_impl(pool.inner(), id, patch).await
}

pub async fn update_task_impl(pool: &SqlitePool, id: String, patch: UpdateTask) -> Result<Task, String> {
    let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
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
    if let Some(category) = patch.category {
        task.category = crate::commands::categories::valid_or_fallback(pool, &category).await;
    }
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

    // Как deadline: пустая строка отвязывает от проекта
    if let Some(pid) = patch.project_id {
        task.project_id = if pid.is_empty() { None } else { Some(pid) };
    }

    // Тайм-блок: пустая строка снимает блок целиком (и длительность)
    let old_scheduled = task.scheduled_at;
    if let Some(sa) = patch.scheduled_at {
        if sa.is_empty() {
            task.scheduled_at = None;
            task.scheduled_mins = None;
        } else {
            task.scheduled_at = Some(DateTime::parse_from_rfc3339(&sa)
                .map_err(|e| e.to_string())?
                .with_timezone(&Utc));
        }
    }
    if let Some(mins) = patch.scheduled_mins {
        task.scheduled_mins = Some(mins.clamp(15, 24 * 60));
    }

    task.updated_at = Utc::now();
    // Если дедлайн реально изменился, старые флаги notified_* больше не
    // отражают актуальный дедлайн — иначе планировщик никогда не пришлёт
    // уведомление по новой дате (раньше это был баг: флаги не сбрасывались).
    let deadline_changed = task.deadline != old_deadline;
    // Перенос блока: сбросить notified_block, чтобы пуш пришёл по новому времени
    let block_changed = task.scheduled_at != old_scheduled;

    sqlx::query(
        "UPDATE tasks SET title=?, description=?, status=?, priority=?,
         category=?, deadline=?, tags=?, recurrence=?, updated_at=?, project_id=?,
         scheduled_at=?, scheduled_mins=?,
         notified_24h = CASE WHEN ? THEN 0 ELSE notified_24h END,
         notified_1h = CASE WHEN ? THEN 0 ELSE notified_1h END,
         notified_deadline = CASE WHEN ? THEN 0 ELSE notified_deadline END,
         notified_block = CASE WHEN ? THEN 0 ELSE notified_block END
         WHERE id=?"
    )
    .bind(&task.title)
    .bind(&task.description)
    .bind(format!("{:?}", task.status))
    .bind(format!("{:?}", task.priority))
    .bind(&task.category)
    .bind(task.deadline.map(|d| d.to_rfc3339()))
    .bind(serde_json::to_string(&task.tags).unwrap_or_else(|_| "[]".into()))
    .bind(task.recurrence.to_db())
    .bind(task.updated_at.to_rfc3339())
    .bind(&task.project_id)
    .bind(task.scheduled_at.map(|d| d.to_rfc3339()))
    .bind(task.scheduled_mins)
    .bind(deadline_changed)
    .bind(deadline_changed)
    .bind(deadline_changed)
    .bind(block_changed)
    .bind(&id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(task)
}

#[tauri::command]
pub async fn complete_task(
  pool: State<'_, SqlitePool>,
  id: String,
) -> Result<Task, String> {
  complete_task_impl(pool.inner(), id).await
}

pub async fn complete_task_impl(pool: &SqlitePool, id: String) -> Result<Task, String> {
  let row = sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
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
      if let Some(scheduled) = &task.scheduled_at {
        task.scheduled_at = Some(*scheduled + duration);
      }
      reset_notifications = true;
    }
  }

  task.updated_at = now;

  sqlx::query(
    "UPDATE tasks SET status=?, hidden=?, deadline=?, completed_at=?, updated_at=?, scheduled_at=?,
     notified_24h = CASE WHEN ? THEN 0 ELSE notified_24h END,
     notified_1h = CASE WHEN ? THEN 0 ELSE notified_1h END,
     notified_deadline = CASE WHEN ? THEN 0 ELSE notified_deadline END,
     notified_block = CASE WHEN ? THEN 0 ELSE notified_block END
     WHERE id=?"
  )
  .bind(format!("{:?}", task.status))
  .bind(task.hidden)
  .bind(task.deadline.map(|d| d.to_rfc3339()))
  .bind(task.completed_at.map(|d| d.to_rfc3339()))
  .bind(task.updated_at.to_rfc3339())
  .bind(task.scheduled_at.map(|d| d.to_rfc3339()))
  .bind(reset_notifications)
  .bind(reset_notifications)
  .bind(reset_notifications)
  .bind(reset_notifications)
  .bind(&id)
  .execute(pool)
  .await
  .map_err(|e| e.to_string())?;

  Ok(task)
}

#[tauri::command]
pub async fn reorder_tasks(pool: State<'_, SqlitePool>, ids: Vec<String>) -> Result<(), String> {
    reorder_tasks_impl(pool.inner(), ids).await
}

// Ручной порядок: фронт присылает id видимого списка в новом порядке.
// Мы переиспользуем те же значения sort_order, что уже были у этих задач,
// раздав их по новому порядку, — задачи вне списка не сдвигаются, а
// коллизий с чужими значениями не возникает.
pub async fn reorder_tasks_impl(pool: &SqlitePool, ids: Vec<String>) -> Result<(), String> {
    if ids.len() < 2 {
        return Ok(());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT id, sort_order FROM tasks WHERE id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, (String, i64)>(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
    let existing: std::collections::HashSet<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    let mut orders: Vec<i64> = rows.iter().map(|(_, o)| *o).collect();
    orders.sort_unstable();

    // Неизвестные id (гонка с удалением) выбрасываем ДО zip — иначе значения
    // раздались бы со сдвигом не тем задачам.
    let live_ids = ids.iter().filter(|id| existing.contains(id.as_str()));
    for (id, ord) in live_ids.zip(orders) {
        sqlx::query("UPDATE tasks SET sort_order = ? WHERE id = ?")
            .bind(ord)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn search_tasks(
  pool: State<'_, SqlitePool>,
  query: String,
) -> Result<Vec<Task>, String> {
  search_tasks_impl(pool.inner(), query).await
}

pub async fn search_tasks_impl(pool: &SqlitePool, query: String) -> Result<Vec<Task>, String> {
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
       AND t.deleted_at IS NULL
     ORDER BY rank"
  )
  .bind(fts_query)
  .fetch_all(pool)
  .await
  .map_err(|e| e.to_string())?;

  let mut tasks: Vec<Task> = rows.into_iter().map(|r| r.into_task()).collect();
  crate::commands::subtasks::attach_subtasks(pool, &mut tasks).await?;
  Ok(tasks)
}

#[derive(Debug, Serialize, Clone)]
pub struct TaskSnippet {
  pub item: Task,
  pub snippet: String,
}

#[tauri::command]
pub async fn search_tasks_snippet(pool: State<'_, SqlitePool>, query: String) -> Result<Vec<TaskSnippet>, String> {
  search_tasks_snippet_impl(pool.inner(), query).await
}

pub async fn search_tasks_snippet_impl(pool: &SqlitePool, query: String) -> Result<Vec<TaskSnippet>, String> {
  let trimmed = query.trim();
  if trimmed.is_empty() {
    return Ok(vec![]);
  }

  let escaped = trimmed.replace('"', "\"\"");
  let fts_query = format!("\"{}\"*", escaped);

  let rows = sqlx::query(
    "SELECT t.*,
            snippet(tasks_fts, 2, '<mark>', '</mark>', '…', 32) AS snippet
     FROM tasks t
     INNER JOIN tasks_fts ON tasks_fts.rowid = t.rowid
     WHERE tasks_fts MATCH ?
       AND t.hidden = 0
       AND t.deleted_at IS NULL
     ORDER BY rank"
  )
  .bind(fts_query)
  .fetch_all(pool)
  .await
  .map_err(|e| e.to_string())?;

  let mut snippets: Vec<TaskSnippet> = Vec::with_capacity(rows.len());
  for row in rows {
    let snippet: Option<String> = row.get("snippet");
    let snippet = snippet.unwrap_or_default();
    let task_row = TaskRow {
      id: row.get("id"),
      title: row.get("title"),
      description: row.get("description"),
      status: row.get("status"),
      priority: row.get("priority"),
      category: row.get("category"),
      deadline: row.get("deadline"),
      tags: row.get("tags"),
      created_at: row.get("created_at"),
      updated_at: row.get("updated_at"),
      completed_at: row.get("completed_at"),
      recurrence: row.get("recurrence"),
      hidden: row.get("hidden"),
      deleted_at: row.get("deleted_at"),
      project_id: row.get("project_id"),
      scheduled_at: row.get("scheduled_at"),
      scheduled_mins: row.get("scheduled_mins"),
      sort_order: row.get("sort_order"),
    };
    let mut task = task_row.into_task();
    task.subtasks = crate::commands::subtasks::get_subtasks_impl(pool, &task.id).await?;
    snippets.push(TaskSnippet { item: task, snippet });
  }
  Ok(snippets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::task::{Priority, Recurrence, RecurrenceUnit};

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn new_task(title: &str) -> CreateTask {
        CreateTask {
            title: title.into(),
            description: Some("desc".into()),
            status: TaskStatus::Todo,
            priority: Priority::Medium,
            category: "Work".into(),
            deadline: Some(Utc::now() + chrono::Duration::days(3)),
            tags: vec!["a".into(), "b".into()],
            recurrence: None,
            project_id: None,
        }
    }

    #[tokio::test]
    async fn create_then_get_roundtrip() {
        let pool = test_pool().await;
        let created = create_task_impl(&pool, new_task("тестовая задача")).await.unwrap();

        let tasks = get_tasks_impl(&pool).await.unwrap();
        assert_eq!(tasks.len(), 1);
        let got = &tasks[0];
        assert_eq!(got.id, created.id);
        assert_eq!(got.title, "тестовая задача");
        assert_eq!(got.tags, vec!["a", "b"]);
        assert_eq!(got.status, TaskStatus::Todo);
        assert!(got.deadline.is_some());
    }

    #[tokio::test]
    async fn create_rejects_empty_title() {
        let pool = test_pool().await;
        assert!(create_task_impl(&pool, new_task("   ")).await.is_err());
    }

    #[tokio::test]
    async fn reorder_permutes_only_given_ids() {
        let pool = test_pool().await;
        let a = create_task_impl(&pool, new_task("а")).await.unwrap();
        let b = create_task_impl(&pool, new_task("б")).await.unwrap();
        let c = create_task_impl(&pool, new_task("в")).await.unwrap();
        let d = create_task_impl(&pool, new_task("г")).await.unwrap();

        // Новые задачи идут в конец: а, б, в, г
        let titles = |tasks: &[Task]| tasks.iter().map(|t| t.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&get_tasks_impl(&pool).await.unwrap()), ["а", "б", "в", "г"]);

        // Переставляем первые три: в, а, б — «г» не трогаем
        reorder_tasks_impl(&pool, vec![c.id.clone(), a.id.clone(), b.id.clone()]).await.unwrap();
        assert_eq!(titles(&get_tasks_impl(&pool).await.unwrap()), ["в", "а", "б", "г"]);

        // Значения sort_order — та же тройка, что была (перестановка, не перенумерация)
        let orders: Vec<i64> = get_tasks_impl(&pool).await.unwrap().iter().map(|t| t.sort_order).collect();
        assert_eq!(orders, [1, 2, 3, 4]);

        // Исчезнувший id не ломает раздачу значений остальным
        delete_task_impl(&pool, a.id.clone()).await.unwrap();
        reorder_tasks_impl(&pool, vec![b.id.clone(), a.id.clone(), c.id.clone()]).await.unwrap();
        assert_eq!(titles(&get_tasks_impl(&pool).await.unwrap()), ["б", "в", "г"]);

        // Один id — no-op без ошибки
        reorder_tasks_impl(&pool, vec![d.id.clone()]).await.unwrap();
    }

    #[tokio::test]
    async fn complete_non_recurring_marks_done_and_hides() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("разовая")).await.unwrap();

        let done = complete_task_impl(&pool, t.id).await.unwrap();
        assert_eq!(done.status, TaskStatus::Done);
        assert!(done.hidden);
        assert!(done.completed_at.is_some());
    }

    #[tokio::test]
    async fn complete_recurring_moves_deadline_and_resets_notifications() {
        let pool = test_pool().await;
        let mut ct = new_task("ежедневная");
        ct.recurrence = Some(Recurrence::Custom(2, RecurrenceUnit::Days));
        let t = create_task_impl(&pool, ct).await.unwrap();

        // Имитируем: планировщик уже уведомил о старом дедлайне
        sqlx::query("UPDATE tasks SET notified_24h = 1, notified_1h = 1 WHERE id = ?")
            .bind(&t.id).execute(&pool).await.unwrap();

        let before = Utc::now();
        let done = complete_task_impl(&pool, t.id.clone()).await.unwrap();

        // Не закрыта, а переехала на +2 дня
        assert_eq!(done.status, TaskStatus::Todo);
        assert!(!done.hidden);
        assert!(done.completed_at.is_none());
        let dl = done.deadline.unwrap();
        assert!(dl >= before + chrono::Duration::days(2));
        assert!(dl <= Utc::now() + chrono::Duration::days(2));

        // Флаги уведомлений сброшены — иначе о новом дедлайне никто не узнает
        let row = sqlx::query_as::<_, (bool, bool)>(
            "SELECT notified_24h, notified_1h FROM tasks WHERE id = ?")
            .bind(&t.id).fetch_one(&pool).await.unwrap();
        assert_eq!(row, (false, false));
    }

    #[tokio::test]
    async fn complete_recurring_shifts_scheduled_block() {
        let pool = test_pool().await;
        let before = Utc::now() - chrono::Duration::minutes(1);
        let scheduled = before + chrono::Duration::hours(2);
        let mut ct = new_task("ежедневная с блоком");
        ct.recurrence = Some(Recurrence::Daily);
        let t = create_task_impl(&pool, ct).await.unwrap();
        sqlx::query("UPDATE tasks SET scheduled_at = ?, scheduled_mins = 30 WHERE id = ?")
            .bind(scheduled.to_rfc3339()).bind(&t.id)
            .execute(&pool).await.unwrap();

        let done = complete_task_impl(&pool, t.id.clone()).await.unwrap();

        // scheduled_at должен сдвинуться на +1 день от исходного, не от now
        let expected = scheduled + chrono::Duration::days(1);
        let got = done.scheduled_at.unwrap();
        assert!((got - expected).num_seconds().abs() < 2,
            "expected {expected}, got {got}");
        assert_eq!(done.scheduled_mins, Some(30)); // не тронут

        // notified_block сброшен (recurring — reset_notifications=true)
        let (block_flag,): (bool,) = sqlx::query_as(
            "SELECT notified_block FROM tasks WHERE id = ?")
            .bind(&t.id).fetch_one(&pool).await.unwrap();
        assert!(!block_flag, "notified_block should be reset");
    }

    #[tokio::test]
    async fn update_deadline_resets_notification_flags() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("с дедлайном")).await.unwrap();
        sqlx::query("UPDATE tasks SET notified_24h = 1 WHERE id = ?")
            .bind(&t.id).execute(&pool).await.unwrap();

        let patch = UpdateTask {
            title: None, description: None, status: None, priority: None,
            category: None, tags: None, recurrence: None, project_id: None,
            scheduled_at: None, scheduled_mins: None,
            deadline: Some((Utc::now() + chrono::Duration::days(10)).to_rfc3339()),
        };
        update_task_impl(&pool, t.id.clone(), patch).await.unwrap();

        let (notified,): (bool,) = sqlx::query_as(
            "SELECT notified_24h FROM tasks WHERE id = ?")
            .bind(&t.id).fetch_one(&pool).await.unwrap();
        assert!(!notified);
    }

    #[tokio::test]
    async fn search_finds_by_prefix_and_survives_hyphen() {
        let pool = test_pool().await;
        create_task_impl(&pool, new_task("купить хлеб-2")).await.unwrap();
        create_task_impl(&pool, new_task("помыть машину")).await.unwrap();

        let found = search_tasks_impl(&pool, "хлеб".into()).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "купить хлеб-2");

        // Дефис — синтаксис FTS5; раньше падало с "no such column"
        let found = search_tasks_impl(&pool, "хлеб-2".into()).await.unwrap();
        assert_eq!(found.len(), 1);

        // Пустой запрос — пустой результат, не ошибка
        assert!(search_tasks_impl(&pool, "  ".into()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_is_soft_hides_from_active_but_keeps_row() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("на удаление")).await.unwrap();
        delete_task_impl(&pool, t.id.clone()).await.unwrap();

        // Не в активных...
        assert!(get_tasks_impl(&pool).await.unwrap().is_empty());
        // ...но строка жива и видна в корзине.
        let trash = get_deleted_tasks_impl(&pool).await.unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].id, t.id);
        assert!(trash[0].deleted_at.is_some());
    }

    #[tokio::test]
    async fn restore_returns_task_to_active_list() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("восстановить")).await.unwrap();
        delete_task_impl(&pool, t.id.clone()).await.unwrap();
        assert!(get_tasks_impl(&pool).await.unwrap().is_empty());

        restore_task_impl(&pool, t.id.clone()).await.unwrap();

        let active = get_tasks_impl(&pool).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, t.id);
        assert_eq!(active[0].deleted_at, None);
        assert!(get_deleted_tasks_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_actually_removes_row_and_unlinks_notes() {
        use crate::commands::notes::{create_note_impl, get_notes_impl, CreateNote};
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("в корзину и навсегда")).await.unwrap();
        create_note_impl(&pool, CreateNote {
            title: "привязанная".into(),
            content: "x".into(),
            tags: vec![],
            linked_task_id: Some(t.id.clone()),
            project_id: None,
        }).await.unwrap();

        delete_task_impl(&pool, t.id.clone()).await.unwrap();
        purge_deleted_task_impl(&pool, t.id.clone()).await.unwrap();

        assert!(get_deleted_tasks_impl(&pool).await.unwrap().is_empty());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id = ?")
            .bind(&t.id).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);

        let notes = get_notes_impl(&pool).await.unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].linked_task_id, None);
    }

    #[tokio::test]
    async fn soft_deleted_task_excluded_from_search() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("искомая задача про хлеб")).await.unwrap();
        assert_eq!(search_tasks_impl(&pool, "хлеб".into()).await.unwrap().len(), 1);

        delete_task_impl(&pool, t.id).await.unwrap();
        assert!(search_tasks_impl(&pool, "хлеб".into()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn schedule_block_set_move_and_clear() {
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("блок")).await.unwrap();
        let start = Utc::now() + chrono::Duration::hours(2);

        let patch = |sa: Option<String>, mins: Option<i64>| UpdateTask {
            title: None, description: None, status: None, priority: None,
            category: None, tags: None, recurrence: None, project_id: None,
            deadline: None, scheduled_at: sa, scheduled_mins: mins,
        };

        // назначить блок
        let up = update_task_impl(&pool, t.id.clone(), patch(Some(start.to_rfc3339()), Some(45))).await.unwrap();
        assert_eq!(up.scheduled_mins, Some(45));
        assert!(up.scheduled_at.is_some());

        // перенос сбрасывает notified_block
        sqlx::query("UPDATE tasks SET notified_block = 1 WHERE id = ?")
            .bind(&t.id).execute(&pool).await.unwrap();
        update_task_impl(&pool, t.id.clone(), patch(Some((start + chrono::Duration::hours(1)).to_rfc3339()), None)).await.unwrap();
        let notified: bool = sqlx::query_scalar("SELECT notified_block FROM tasks WHERE id = ?")
            .bind(&t.id).fetch_one(&pool).await.unwrap();
        assert!(!notified);

        // длительность зажимается снизу
        let up = update_task_impl(&pool, t.id.clone(), patch(None, Some(5))).await.unwrap();
        assert_eq!(up.scheduled_mins, Some(15));

        // пустая строка снимает блок целиком
        let up = update_task_impl(&pool, t.id.clone(), patch(Some(String::new()), None)).await.unwrap();
        assert_eq!(up.scheduled_at, None);
        assert_eq!(up.scheduled_mins, None);
    }

    #[tokio::test]
    async fn soft_delete_keeps_note_link_intact() {
        // v0.8.12: мягкое удаление НЕ трогает привязки заметок/подзадач —
        // это отличие от purge_deleted_task (см. purge_actually_removes_row_and_unlinks_notes).
        use crate::commands::notes::{create_note_impl, get_notes_impl, CreateNote};
        let pool = test_pool().await;
        let t = create_task_impl(&pool, new_task("с заметкой")).await.unwrap();
        create_note_impl(&pool, CreateNote {
            title: "привязанная".into(),
            content: "x".into(),
            tags: vec![],
            linked_task_id: Some(t.id.clone()),
            project_id: None,
        }).await.unwrap();

        delete_task_impl(&pool, t.id.clone()).await.unwrap();

        let notes = get_notes_impl(&pool).await.unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].linked_task_id, Some(t.id));
    }
}
