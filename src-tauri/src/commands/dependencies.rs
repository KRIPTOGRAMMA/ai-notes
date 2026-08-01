// Зависимости задач (v0.9.56): «Б заблокирована задачей А».
//
// Два решения пользователя, определившие эту логику:
//  1. Блокер в Корзине НЕ блокирует, но связь сохраняется и возвращается
//     вместе с ним при восстановлении. Корзина мягкая (tasks.deleted_at),
//     поэтому терять зависимость насовсем нельзя — иначе восстановление
//     задачи молча теряет часть смысла.
//  2. Заблокированную задачу нельзя выполнить (не просто приглушить).
//     Запрет живёт в complete_task, здесь — только источник правды о том,
//     кто кем заблокирован.
use tauri::State;
use sqlx::SqlitePool;
use crate::core::task::{Blocker, Task};

/// Блокером считается задача, которая ещё не закрыта и не в Корзине.
/// Одно условие на оба места, где нужен этот вопрос, — список задач и
/// проверка при выполнении.
const OPEN_BLOCKER: &str = "b.completed_at IS NULL AND b.hidden = 0 AND b.deleted_at IS NULL";

/// Проставляет `blocked_by` пачке задач одним запросом (тот же приём, что
/// attach_subtasks): иначе на списке из N задач вышло бы N запросов.
pub async fn attach_blockers(pool: &SqlitePool, tasks: &mut [Task]) -> Result<(), String> {
  if tasks.is_empty() {
    return Ok(());
  }
  let rows = sqlx::query_as::<_, (String, String, String)>(&format!(
    "SELECT d.task_id, b.id, b.title
     FROM task_dependencies d
     JOIN tasks b ON b.id = d.blocker_id
     WHERE {OPEN_BLOCKER}
     ORDER BY b.title"
  ))
  .fetch_all(pool)
  .await
  .map_err(|e| e.to_string())?;

  for task in tasks.iter_mut() {
    task.blocked_by = rows
      .iter()
      .filter(|(task_id, _, _)| *task_id == task.id)
      .map(|(_, id, title)| Blocker { id: id.clone(), title: title.clone() })
      .collect();
  }
  Ok(())
}

/// Незакрытые блокеры одной задачи. Используется в complete_task для запрета
/// выполнения и во фронтенде после точечных изменений.
pub async fn blockers_of(pool: &SqlitePool, task_id: &str) -> Result<Vec<Blocker>, String> {
  sqlx::query_as::<_, Blocker>(&format!(
    "SELECT b.id, b.title
     FROM task_dependencies d
     JOIN tasks b ON b.id = d.blocker_id
     WHERE d.task_id = ? AND {OPEN_BLOCKER}
     ORDER BY b.title"
  ))
  .bind(task_id)
  .fetch_all(pool)
  .await
  .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_task_blockers(
  pool: State<'_, SqlitePool>,
  task_id: String,
) -> Result<Vec<Blocker>, String> {
  blockers_of(pool.inner(), &task_id).await
}

#[tauri::command]
pub async fn add_task_dependency(
  pool: State<'_, SqlitePool>,
  task_id: String,
  blocker_id: String,
) -> Result<(), String> {
  add_task_dependency_impl(pool.inner(), &task_id, &blocker_id).await
}

pub async fn add_task_dependency_impl(
  pool: &SqlitePool,
  task_id: &str,
  blocker_id: &str,
) -> Result<(), String> {
  if task_id == blocker_id {
    return Err("Задача не может блокировать саму себя".into());
  }
  // Цикл (А ждёт Б, Б ждёт А) навсегда заблокировал бы обе задачи: ни одну
  // нельзя выполнить, а значит и разблокировать другую. Проверяем до записи.
  if depends_on(pool, blocker_id, task_id).await? {
    return Err("Циклическая зависимость: эта задача уже блокирует выбранную".into());
  }
  sqlx::query(
    "INSERT OR IGNORE INTO task_dependencies (task_id, blocker_id, created_at) VALUES (?, ?, ?)"
  )
  .bind(task_id)
  .bind(blocker_id)
  .bind(chrono::Utc::now().to_rfc3339())
  .execute(pool)
  .await
  .map_err(|e| e.to_string())?;
  Ok(())
}

/// Есть ли путь «from ждёт ... ждёт to» по всей цепочке зависимостей.
/// Обход в ширину: цепочка длиннее одного звена тоже даёт цикл (А→Б→В→А).
/// Удалённые блокеры здесь НЕ отфильтрованы намеренно: связь с задачей в
/// Корзине жива и вернётся при восстановлении, поэтому цикл через неё —
/// такой же цикл, просто отложенный.
async fn depends_on(pool: &SqlitePool, from: &str, to: &str) -> Result<bool, String> {
  let mut seen = std::collections::HashSet::new();
  let mut queue = vec![from.to_string()];
  while let Some(current) = queue.pop() {
    if !seen.insert(current.clone()) {
      continue;
    }
    let blockers = sqlx::query_scalar::<_, String>(
      "SELECT blocker_id FROM task_dependencies WHERE task_id = ?"
    )
    .bind(&current)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for b in blockers {
      if b == to {
        return Ok(true);
      }
      queue.push(b);
    }
  }
  Ok(false)
}

#[tauri::command]
pub async fn remove_task_dependency(
  pool: State<'_, SqlitePool>,
  task_id: String,
  blocker_id: String,
) -> Result<(), String> {
  remove_task_dependency_impl(pool.inner(), &task_id, &blocker_id).await
}

pub async fn remove_task_dependency_impl(
  pool: &SqlitePool,
  task_id: &str,
  blocker_id: &str,
) -> Result<(), String> {
  sqlx::query("DELETE FROM task_dependencies WHERE task_id = ? AND blocker_id = ?")
    .bind(task_id)
    .bind(blocker_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
  Ok(())
}

/// Пишет в Центр уведомлений «теперь можно взяться» по каждой задаче, которую
/// разблокировало закрытие `blocker_id`.
///
/// Пишем прямо в notification_log, а не через notifier::send_notification:
/// тот требует AppHandle, которого в командах задач нет, и протаскивать его
/// через complete_task ради этого пришлось бы во все места, откуда задачу
/// можно закрыть (трей, быстрый слот, палитра). Запись в ленту даёт то же
/// самое там, где пользователь её и увидит, и кликом откроет задачу.
pub async fn notify_unblocked(pool: &SqlitePool, blocker_id: &str) -> Result<(), String> {
  let lang = crate::i18n::current_lang(pool).await;
  for task in unblocked_by(pool, blocker_id).await? {
    let title = crate::i18n::tr("Задача разблокирована", lang);
    let body = crate::i18n::tr_args(
      "Можно взяться: {task}.",
      lang,
      &[("task", task.title.clone())],
    );
    let _ = sqlx::query(
      "INSERT INTO notification_log (id, kind, title, body, created_at, entity_type, entity_id)
       VALUES (?, 'unblocked', ?, ?, ?, 'task', ?)"
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&title)
    .bind(&body)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&task.id)
    .execute(pool)
    .await;
  }
  Ok(())
}

/// Задачи, которые разблокируются закрытием этой (т.е. она была их
/// последним незакрытым блокером). Нужно для уведомления «теперь можно».
pub async fn unblocked_by(pool: &SqlitePool, blocker_id: &str) -> Result<Vec<Blocker>, String> {
  sqlx::query_as::<_, Blocker>(&format!(
    "SELECT t.id, t.title
     FROM task_dependencies d
     JOIN tasks t ON t.id = d.task_id
     WHERE d.blocker_id = ? AND t.deleted_at IS NULL AND t.completed_at IS NULL
       AND NOT EXISTS (
         SELECT 1 FROM task_dependencies d2
         JOIN tasks b ON b.id = d2.blocker_id
         WHERE d2.task_id = d.task_id AND d2.blocker_id != ? AND {OPEN_BLOCKER}
       )
     ORDER BY t.title"
  ))
  .bind(blocker_id)
  .bind(blocker_id)
  .fetch_all(pool)
  .await
  .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::tasks::{
    complete_task_impl, delete_task_impl, get_tasks_impl, restore_task_impl,
  };
  use crate::core::task::{CreateTask, Priority};

  async fn test_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
    pool
  }

  async fn task(pool: &SqlitePool, title: &str) -> String {
    crate::commands::tasks::create_task_impl(pool, CreateTask {
      title: title.into(),
      description: None,
      status: "Todo".into(),
      priority: Priority::Medium,
      category: "Work".into(),
      deadline: None,
      tags: vec![],
      recurrence: None,
      project_id: None,
    })
    .await
    .unwrap()
    .id
  }

  #[tokio::test]
  async fn blocked_task_reports_its_blocker() {
    let pool = test_pool().await;
    let a = task(&pool, "фундамент").await;
    let b = task(&pool, "стены").await;
    add_task_dependency_impl(&pool, &b, &a).await.unwrap();

    let tasks = get_tasks_impl(&pool).await.unwrap();
    let walls = tasks.iter().find(|t| t.id == b).unwrap();
    assert_eq!(walls.blocked_by.len(), 1);
    assert_eq!(walls.blocked_by[0].title, "фундамент");
    // Блокер сам ничем не заблокирован
    assert!(tasks.iter().find(|t| t.id == a).unwrap().blocked_by.is_empty());
  }

  #[tokio::test]
  async fn blocked_task_cannot_be_completed() {
    let pool = test_pool().await;
    let a = task(&pool, "фундамент").await;
    let b = task(&pool, "стены").await;
    add_task_dependency_impl(&pool, &b, &a).await.unwrap();

    let err = complete_task_impl(&pool, b.clone()).await.unwrap_err();
    assert!(err.contains("фундамент"), "в ошибке должно быть имя блокера: {err}");

    // Закрыли блокер — задача освободилась
    complete_task_impl(&pool, a).await.unwrap();
    assert!(blockers_of(&pool, &b).await.unwrap().is_empty());
    complete_task_impl(&pool, b).await.unwrap();
  }

  // Решение пользователя: Корзина мягкая, поэтому блокер в ней не блокирует,
  // но связь жива и возвращается вместе с задачей при восстановлении.
  #[tokio::test]
  async fn trashed_blocker_unblocks_but_link_survives_restore() {
    let pool = test_pool().await;
    let a = task(&pool, "фундамент").await;
    let b = task(&pool, "стены").await;
    add_task_dependency_impl(&pool, &b, &a).await.unwrap();

    delete_task_impl(&pool, a.clone()).await.unwrap();
    assert!(
      blockers_of(&pool, &b).await.unwrap().is_empty(),
      "блокер в Корзине не должен блокировать"
    );

    restore_task_impl(&pool, a.clone()).await.unwrap();
    let back = blockers_of(&pool, &b).await.unwrap();
    assert_eq!(back.len(), 1, "связь должна вернуться вместе с задачей");
    assert_eq!(back[0].id, a);
  }

  #[tokio::test]
  async fn several_blockers_release_one_by_one() {
    let pool = test_pool().await;
    let a = task(&pool, "проект").await;
    let b = task(&pool, "смета").await;
    let c = task(&pool, "стройка").await;
    add_task_dependency_impl(&pool, &c, &a).await.unwrap();
    add_task_dependency_impl(&pool, &c, &b).await.unwrap();
    assert_eq!(blockers_of(&pool, &c).await.unwrap().len(), 2);

    complete_task_impl(&pool, a).await.unwrap();
    assert_eq!(
      blockers_of(&pool, &c).await.unwrap().len(),
      1,
      "один закрытый блокер из двух не разблокирует"
    );
    complete_task_impl(&pool, b).await.unwrap();
    assert!(blockers_of(&pool, &c).await.unwrap().is_empty());
  }

  #[tokio::test]
  async fn cycles_are_rejected() {
    let pool = test_pool().await;
    let a = task(&pool, "а").await;
    let b = task(&pool, "б").await;
    let c = task(&pool, "в").await;

    assert!(add_task_dependency_impl(&pool, &a, &a).await.is_err(), "сама на себя");

    add_task_dependency_impl(&pool, &b, &a).await.unwrap();
    assert!(add_task_dependency_impl(&pool, &a, &b).await.is_err(), "прямой цикл");

    // Цепочка длиннее одного звена: в ждёт б, б ждёт а => а не может ждать в
    add_task_dependency_impl(&pool, &c, &b).await.unwrap();
    assert!(add_task_dependency_impl(&pool, &a, &c).await.is_err(), "цикл через цепочку");
  }

  #[tokio::test]
  async fn completing_last_blocker_notifies_unblocked_task() {
    let pool = test_pool().await;
    let a = task(&pool, "фундамент").await;
    let b = task(&pool, "смета").await;
    let c = task(&pool, "стены").await;
    add_task_dependency_impl(&pool, &c, &a).await.unwrap();
    add_task_dependency_impl(&pool, &c, &b).await.unwrap();

    // Первый из двух блокеров задачу ещё не освобождает — уведомления нет
    assert!(unblocked_by(&pool, &a).await.unwrap().is_empty());

    complete_task_impl(&pool, a).await.unwrap();
    let freed = unblocked_by(&pool, &b).await.unwrap();
    assert_eq!(freed.len(), 1, "второй блокер закрывает последнюю зависимость");
    assert_eq!(freed[0].id, c);

    complete_task_impl(&pool, b).await.unwrap();
    let feed = crate::commands::notifications::get_notification_log_impl(&pool).await.unwrap();
    assert!(
      feed.iter().any(|n| n.entity_id.as_deref() == Some(c.as_str())),
      "в ленте должно появиться уведомление о разблокировке"
    );
  }

  #[tokio::test]
  async fn removing_dependency_frees_task() {
    let pool = test_pool().await;
    let a = task(&pool, "фундамент").await;
    let b = task(&pool, "стены").await;
    add_task_dependency_impl(&pool, &b, &a).await.unwrap();
    remove_task_dependency_impl(&pool, &b, &a).await.unwrap();
    assert!(blockers_of(&pool, &b).await.unwrap().is_empty());
    complete_task_impl(&pool, b).await.unwrap();
  }
}
