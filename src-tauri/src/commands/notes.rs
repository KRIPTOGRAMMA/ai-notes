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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub content: Option<String>,
}

fn row_to_note(row: sqlx::sqlite::SqliteRow) -> Note {
    Note {
        id: row.get("id"),
        title: row.get("title"),
        content: row.get("content"),
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
        "SELECT id, title, content, created_at, updated_at FROM notes ORDER BY updated_at DESC"
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

    sqlx::query(
        "INSERT INTO notes (id, title, content, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&title)
    .bind(&note.content)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(Note { id, title, content: note.content, created_at: now.clone(), updated_at: now })
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

    let row = sqlx::query(
        "SELECT id, title, content, created_at, updated_at FROM notes WHERE id = ?"
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
        }).await.unwrap();

        let all = get_notes_impl(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "заметка");

        let updated = update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None,
            content: Some("новый текст".into()),
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
        }).await.unwrap();
        assert_eq!(note.title, "Без названия");
    }
}
