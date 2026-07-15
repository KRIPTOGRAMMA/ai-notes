use tauri::State;
use sqlx::{SqlitePool, Row};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::AppResult;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub linked_task_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub linked_task_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    // Some(Some(id)) — привязать, Some(None) — отвязать, None — не трогать.
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub linked_task_id: Option<Option<String>>,
    // Аналогично: Some(Some(id)) — в проект, Some(None) — из проекта, None — не трогать.
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub project_id: Option<Option<String>>,
}

// Различаем «поле отсутствует» и «поле = null» в JSON: нужно, чтобы отвязку
// (linked_task_id: null) отличать от «не трогать» (поле не прислано).
fn deserialize_optional_field<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

fn row_to_note(row: sqlx::sqlite::SqliteRow) -> Note {
    let tags_json: String = row.get("tags");
    Note {
        id: row.get("id"),
        title: row.get("title"),
        content: row.get("content"),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        linked_task_id: row.get("linked_task_id"),
        project_id: row.get("project_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[tauri::command]
pub async fn get_notes(pool: State<'_, SqlitePool>) -> AppResult<Vec<Note>> {
    get_notes_impl(pool.inner()).await
}

pub async fn get_notes_impl(pool: &SqlitePool) -> AppResult<Vec<Note>> {
    let rows = sqlx::query(
        "SELECT id, title, content, tags, linked_task_id, project_id, created_at, updated_at FROM notes ORDER BY updated_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_note).collect())
}

#[tauri::command]
pub async fn create_note(pool: State<'_, SqlitePool>, note: CreateNote) -> AppResult<Note> {
    create_note_impl(pool.inner(), note).await
}

pub async fn create_note_impl(pool: &SqlitePool, note: CreateNote) -> AppResult<Note> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let title = if note.title.trim().is_empty() { "Без названия".to_string() } else { note.title };
    let tags_json = serde_json::to_string(&note.tags).unwrap_or_else(|_| "[]".into());

    sqlx::query(
        "INSERT INTO notes (id, title, content, tags, linked_task_id, project_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&title)
    .bind(&note.content)
    .bind(&tags_json)
    .bind(&note.linked_task_id)
    .bind(&note.project_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(Note {
        id,
        title,
        content: note.content,
        tags: note.tags,
        linked_task_id: note.linked_task_id,
        project_id: note.project_id,
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub async fn update_note(
    pool: State<'_, SqlitePool>,
    id: String,
    patch: UpdateNote,
) -> AppResult<Note> {
    update_note_impl(pool.inner(), id, patch).await
}

pub async fn update_note_impl(pool: &SqlitePool, id: String, patch: UpdateNote) -> AppResult<Note> {
    let now = Utc::now().to_rfc3339();

    if let Some(ref title) = patch.title {
        sqlx::query("UPDATE notes SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title).bind(&now).bind(&id)
            .execute(pool).await?;
    }
    if let Some(ref content) = patch.content {
        sqlx::query("UPDATE notes SET content = ?, updated_at = ? WHERE id = ?")
            .bind(content).bind(&now).bind(&id)
            .execute(pool).await?;
    }
    if let Some(ref tags) = patch.tags {
        let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
        sqlx::query("UPDATE notes SET tags = ?, updated_at = ? WHERE id = ?")
            .bind(&tags_json).bind(&now).bind(&id)
            .execute(pool).await?;
    }
    if let Some(ref linked) = patch.linked_task_id {
        // linked: Some(id) — привязать, None — отвязать (linked сам Option<String>)
        sqlx::query("UPDATE notes SET linked_task_id = ?, updated_at = ? WHERE id = ?")
            .bind(linked).bind(&now).bind(&id)
            .execute(pool).await?;
    }
    if let Some(ref project) = patch.project_id {
        sqlx::query("UPDATE notes SET project_id = ?, updated_at = ? WHERE id = ?")
            .bind(project).bind(&now).bind(&id)
            .execute(pool).await?;
    }

    let row = sqlx::query(
        "SELECT id, title, content, tags, linked_task_id, project_id, created_at, updated_at FROM notes WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(pool)
    .await?;

    Ok(row_to_note(row))
}

#[tauri::command]
pub async fn delete_note(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_note_impl(pool.inner(), id).await
}

pub async fn delete_note_impl(pool: &SqlitePool, id: String) -> AppResult<()> {
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(&id)
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
    async fn create_get_update_delete_roundtrip() {
        let pool = test_pool().await;

        let note = create_note_impl(&pool, CreateNote {
            title: "заметка".into(),
            content: "текст".into(),
            tags: vec![],
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();

        let all = get_notes_impl(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "заметка");

        let updated = update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None,
            content: Some("новый текст".into()),
            tags: None,
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();
        assert_eq!(updated.content, "новый текст");
        assert_eq!(updated.title, "заметка"); // не тронут

        delete_note_impl(&pool, note.id).await.unwrap();
        assert!(get_notes_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_title_becomes_placeholder() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "   ".into(),
            content: "x".into(),
            tags: vec![],
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();
        assert_eq!(note.title, "Без названия");
    }

    #[tokio::test]
    async fn tags_and_link_roundtrip() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "с тегами".into(),
            content: "x".into(),
            tags: vec!["work".into(), "idea".into()],
            linked_task_id: Some("task-1".into()),
            project_id: Some("proj-1".into()),
        }).await.unwrap();
        assert_eq!(note.tags, vec!["work", "idea"]);
        assert_eq!(note.linked_task_id.as_deref(), Some("task-1"));

        // Перечитали из БД — сериализация/парсинг тегов и привязки сохранились
        let all = get_notes_impl(&pool).await.unwrap();
        assert_eq!(all[0].tags, vec!["work", "idea"]);
        assert_eq!(all[0].linked_task_id.as_deref(), Some("task-1"));

        // Обновление тегов и отвязка (Some(None))
        let updated = update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None,
            content: None,
            tags: Some(vec!["done".into()]),
            linked_task_id: Some(None),
            project_id: Some(None),
        }).await.unwrap();
        assert_eq!(updated.tags, vec!["done"]);
        assert_eq!(updated.linked_task_id, None);
        assert_eq!(updated.project_id, None);
    }
}
