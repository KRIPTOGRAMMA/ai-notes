// v0.9.33: «быстрый слот» — одна закреплённая задача или заметка, которую
// глобальный хоткей открывает сразу на правку текста, без списка и поиска.
//
// Хранение — две строки в settings (`pinned_kind`, `pinned_id`), а не своя
// таблица: слот ровно один, у него нет ни истории, ни связей, ни порядка.
// Таблица на одну строку здесь была бы миграцией ради ключа-значения,
// которое уже есть.
use crate::error::AppResult;
use crate::commands::settings::{get_setting, set_setting};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

// Что лежит в слоте. `text` — description для задачи и content для заметки:
// окно правит только текст, поэтому одно поле вместо двух разных.
#[derive(Debug, Serialize, PartialEq)]
pub struct PinnedItem {
    pub kind: String, // "task" | "note"
    pub id: String,
    pub title: String,
    pub text: String,
}

// Нормализация вида. Чужая строка — не ошибка, а пустой слот: значение может
// прийти из БД, отредактированной руками, и падать из-за этого нельзя.
pub fn normalize_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "task" => Some("task"),
        "note" => Some("note"),
        _ => None,
    }
}

// Чтение слота. None во всех «нечего показывать» случаях, и это не ошибка:
// слот пуст, вид неизвестен, id пустой, объект удалён.
//
// Про удаление важно, что оно у задач и заметок разное: задача уходит в
// Корзину (`deleted_at` проставлен, строка на месте), заметка удаляется
// строкой из таблицы. Поэтому у задачи мало проверить существование —
// нужен ещё и фильтр по `deleted_at`, иначе хоткей открыл бы на правку то,
// что пользователь выбросил.
pub async fn get_pinned_impl(pool: &SqlitePool) -> AppResult<Option<PinnedItem>> {
    let kind = match get_setting(pool, "pinned_kind").await.and_then(|k| {
        normalize_kind(&k).map(|s| s.to_string())
    }) {
        Some(k) => k,
        None => return Ok(None),
    };
    let id = match get_setting(pool, "pinned_id").await {
        Some(id) if !id.trim().is_empty() => id,
        _ => return Ok(None),
    };

    let row: Option<(String, Option<String>)> = if kind == "task" {
        sqlx::query_as("SELECT title, description FROM tasks WHERE id = ? AND deleted_at IS NULL")
            .bind(&id)
            .fetch_optional(pool)
            .await?
    } else {
        sqlx::query_as("SELECT title, content FROM notes WHERE id = ?")
            .bind(&id)
            .fetch_optional(pool)
            .await?
    };

    Ok(row.map(|(title, text)| PinnedItem {
        kind,
        id,
        title,
        text: text.unwrap_or_default(),
    }))
}

// Запись слота. kind = None очищает слот (кнопка «открепить»).
pub async fn set_pinned_impl(
    pool: &SqlitePool,
    kind: Option<String>,
    id: Option<String>,
) -> AppResult<()> {
    let valid = kind
        .as_deref()
        .and_then(normalize_kind)
        .zip(id.as_deref().filter(|s| !s.trim().is_empty()));

    match valid {
        Some((k, id)) => {
            set_setting(pool, "pinned_kind", k).await?;
            set_setting(pool, "pinned_id", id).await?;
        }
        None => {
            set_setting(pool, "pinned_kind", "").await?;
            set_setting(pool, "pinned_id", "").await?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pinned_item(pool: State<'_, SqlitePool>) -> AppResult<Option<PinnedItem>> {
    get_pinned_impl(pool.inner()).await
}

#[tauri::command]
pub async fn set_pinned_item(
    pool: State<'_, SqlitePool>,
    kind: Option<String>,
    id: Option<String>,
) -> AppResult<()> {
    set_pinned_impl(pool.inner(), kind, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_kind_accepts_only_known() {
        assert_eq!(normalize_kind("task"), Some("task"));
        assert_eq!(normalize_kind("note"), Some("note"));
        // Мусор в БД не должен ронять чтение слота — просто «пусто».
        assert_eq!(normalize_kind("clipboard"), None);
        assert_eq!(normalize_kind(""), None);
        assert_eq!(normalize_kind("Task"), None);
    }

    use crate::core::task::{CreateTask, Priority};

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn new_task(title: &str, description: &str) -> CreateTask {
        CreateTask {
            title: title.into(),
            description: Some(description.into()),
            status: "Todo".into(),
            priority: Priority::Medium,
            category: "Work".into(),
            deadline: None,
            tags: vec![],
            recurrence: None,
            project_id: None,
        }
    }

    fn new_note(title: &str, content: &str) -> crate::commands::notes::CreateNote {
        crate::commands::notes::CreateNote {
            title: title.into(),
            content: content.into(),
            tags: vec![],
            linked_task_id: None,
            project_id: None,
        }
    }

    #[tokio::test]
    async fn empty_slot_reads_as_none() {
        let pool = test_pool().await;
        assert_eq!(get_pinned_impl(&pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn pinned_task_round_trip() {
        let pool = test_pool().await;
        let task = crate::commands::tasks::create_task_impl(
            &pool,
            new_task("Дописать главу", "план на вечер"),
        )
        .await
        .unwrap();

        set_pinned_impl(&pool, Some("task".into()), Some(task.id.clone()))
            .await
            .unwrap();

        let got = get_pinned_impl(&pool).await.unwrap().unwrap();
        assert_eq!(got.kind, "task");
        assert_eq!(got.id, task.id);
        assert_eq!(got.title, "Дописать главу");
        assert_eq!(got.text, "план на вечер");
    }

    // Задача в Корзине не должна открываться хоткеем: пользователь её выбросил.
    // Проверяется отдельно от заметок, потому что удаление у них разное —
    // здесь строка остаётся в таблице, и без фильтра по deleted_at слот бы «жил».
    #[tokio::test]
    async fn trashed_task_reads_as_empty_slot() {
        let pool = test_pool().await;
        let task = crate::commands::tasks::create_task_impl(
            &pool,
            new_task("В корзину", "текст"),
        )
        .await
        .unwrap();
        set_pinned_impl(&pool, Some("task".into()), Some(task.id.clone()))
            .await
            .unwrap();

        crate::commands::tasks::delete_task_impl(&pool, task.id.clone())
            .await
            .unwrap();

        assert_eq!(get_pinned_impl(&pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleted_note_reads_as_empty_slot() {
        let pool = test_pool().await;
        let note = crate::commands::notes::create_note_impl(&pool, new_note("Заметка", "текст"))
            .await
            .unwrap();
        set_pinned_impl(&pool, Some("note".into()), Some(note.id.clone()))
            .await
            .unwrap();

        crate::commands::notes::delete_note_impl(&pool, note.id.clone())
            .await
            .unwrap();

        assert_eq!(get_pinned_impl(&pool).await.unwrap(), None);
    }

    #[tokio::test]
    async fn unpin_clears_slot() {
        let pool = test_pool().await;
        let note = crate::commands::notes::create_note_impl(&pool, new_note("Заметка", "текст"))
            .await
            .unwrap();
        set_pinned_impl(&pool, Some("note".into()), Some(note.id.clone()))
            .await
            .unwrap();
        assert!(get_pinned_impl(&pool).await.unwrap().is_some());

        set_pinned_impl(&pool, None, None).await.unwrap();
        assert_eq!(get_pinned_impl(&pool).await.unwrap(), None);
    }
}
