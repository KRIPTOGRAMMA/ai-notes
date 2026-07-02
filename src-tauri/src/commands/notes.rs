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
    let rows = sqlx::query(
        "SELECT id, title, content, created_at, updated_at FROM notes ORDER BY updated_at DESC"
    )
    .fetch_all(pool.inner())
    .await?;

    Ok(rows.into_iter().map(row_to_note).collect())
}

#[tauri::command]
pub async fn create_note(pool: State<'_, SqlitePool>, note: CreateNote) -> AppResult<Note> {
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
    .execute(pool.inner())
    .await?;

    Ok(Note { id, title, content: note.content, created_at: now.clone(), updated_at: now })
}

#[tauri::command]
pub async fn update_note(
    pool: State<'_, SqlitePool>,
    id: String,
    patch: UpdateNote,
) -> AppResult<Note> {
    let now = Utc::now().to_rfc3339();

    if let Some(ref title) = patch.title {
        sqlx::query("UPDATE notes SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title).bind(&now).bind(&id)
            .execute(pool.inner()).await?;
    }
    if let Some(ref content) = patch.content {
        sqlx::query("UPDATE notes SET content = ?, updated_at = ? WHERE id = ?")
            .bind(content).bind(&now).bind(&id)
            .execute(pool.inner()).await?;
    }

    let row = sqlx::query(
        "SELECT id, title, content, created_at, updated_at FROM notes WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(pool.inner())
    .await?;

    Ok(row_to_note(row))
}

#[tauri::command]
pub async fn delete_note(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(&id)
        .execute(pool.inner())
        .await?;
    Ok(())
}
