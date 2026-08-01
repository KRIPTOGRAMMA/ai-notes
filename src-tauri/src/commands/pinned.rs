// The "quick slot": a single pinned task or note that a global hotkey opens
// straight into text editing, with no list and no search.
//
// Stored as two rows in settings (`pinned_kind`, `pinned_id`) rather than in a
// table of its own: there is exactly one slot, with no history, no relations and
// no ordering. A one-row table here would be a migration for the sake of a
// key-value pair that already exists.
use crate::error::AppResult;
use crate::commands::settings::{get_setting, set_setting};
use crate::commands::subtasks::get_subtasks_impl;
use crate::core::task::Subtask;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

// What sits in the slot. `text` is the description for a task and the content
// for a note: the window edits text only, hence one field instead of two.
//
// A task carries its checklist along with the text. For a note it is always
// empty — only tasks have subtasks, and there is no point in a separate
// placeholder field: an empty Vec reads the same either way.
#[derive(Debug, Serialize, PartialEq)]
pub struct PinnedItem {
    pub kind: String, // "task" | "note"
    pub id: String,
    pub title: String,
    pub text: String,
    pub subtasks: Vec<Subtask>,
}

// Normalizes the kind. An unrecognized string is not an error but an empty
// slot: the value may come from a hand-edited DB, and crashing over that is not
// acceptable.
pub fn normalize_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "task" => Some("task"),
        "note" => Some("note"),
        _ => None,
    }
}

// Reads the slot. None in every "nothing to show" case, and that is not an
// error: the slot is empty, the kind is unknown, the id is empty, the object was
// deleted.
//
// What matters about deletion is that it differs between tasks and notes: a task
// goes to the Trash (`deleted_at` is set, the row stays), while a note is
// removed from the table outright. So checking that a task exists is not enough
// — a `deleted_at` filter is needed too, otherwise the hotkey would open for
// editing something the user has thrown away.
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

    let (title, text) = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    // The checklist is fetched with the same query used everywhere else in the
    // app (`get_subtasks_impl`) rather than a private SELECT: subtask ordering is
    // defined in one place, or the slot would one day show them in a different
    // order than the task list does.
    let subtasks = if kind == "task" {
        get_subtasks_impl(pool, &id).await?
    } else {
        vec![]
    };

    Ok(Some(PinnedItem {
        kind,
        id,
        title,
        text: text.unwrap_or_default(),
        subtasks,
    }))
}

// Writes the slot. kind = None clears it (the "unpin" button).
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
        // Junk in the DB must not break reading the slot — it is simply empty.
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
        // A task with no checklist yields an empty list, not a missing field.
        assert!(got.subtasks.is_empty());
    }

    // The checklist arrives together with the slot — otherwise the editing
    // window would know less about the task than the task list does, and the
    // subtasks would have to be fetched by a second request after rendering.
    #[tokio::test]
    async fn pinned_task_carries_its_subtasks_in_order() {
        let pool = test_pool().await;
        let task = crate::commands::tasks::create_task_impl(&pool, new_task("Отчёт", "текст"))
            .await
            .unwrap();
        crate::commands::subtasks::add_subtask_impl(&pool, &task.id, "собрать цифры")
            .await
            .unwrap();
        let second =
            crate::commands::subtasks::add_subtask_impl(&pool, &task.id, "отправить")
                .await
                .unwrap();
        crate::commands::subtasks::toggle_subtask_impl(&pool, &second.id)
            .await
            .unwrap();

        // An unrelated task with its own checklist: its subtasks must not leak into the slot.
        let other = crate::commands::tasks::create_task_impl(&pool, new_task("Другая", ""))
            .await
            .unwrap();
        crate::commands::subtasks::add_subtask_impl(&pool, &other.id, "не сюда")
            .await
            .unwrap();

        set_pinned_impl(&pool, Some("task".into()), Some(task.id.clone()))
            .await
            .unwrap();

        let got = get_pinned_impl(&pool).await.unwrap().unwrap();
        let titles: Vec<&str> = got.subtasks.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["собрать цифры", "отправить"]);
        assert!(!got.subtasks[0].done);
        assert!(got.subtasks[1].done);
    }

    // A note never has subtasks: the field exists but is always empty. Checked so
    // the frontend can render the checklist from one and the same field without
    // branching on whether it is there at all.
    #[tokio::test]
    async fn pinned_note_has_no_subtasks() {
        let pool = test_pool().await;
        let note = crate::commands::notes::create_note_impl(&pool, new_note("Заметка", "текст"))
            .await
            .unwrap();
        set_pinned_impl(&pool, Some("note".into()), Some(note.id.clone()))
            .await
            .unwrap();

        let got = get_pinned_impl(&pool).await.unwrap().unwrap();
        assert!(got.subtasks.is_empty());
    }

    // A task in the Trash must not open by hotkey: the user threw it away.
    // Checked separately from notes because deletion differs between them — here
    // the row stays in the table, and without a deleted_at filter the slot would
    // keep "living".
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
