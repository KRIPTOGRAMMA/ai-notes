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

// Вики-ссылка на переименованную заметку: [[old]] или [[old|алиас]] → [[new]] /
// [[new|алиас]] — только цель меняется, алиас (если был) остаётся как есть.
// Регистронезависимо; заголовок в [[...]] может содержать любые символы кроме
// '[', ']', '|' (см. WIKILINK_RE в src/lib/markdown.ts — зеркалим тот же формат).
fn rewrite_links(content: &str, old_title: &str, new_title: &str) -> (String, bool) {
    let old_lower = old_title.to_lowercase();
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    let mut rest = content;

    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            // Незакрытая ссылка до конца строки — остаток копируем как есть
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let inner = &after[..end];
        let (target, alias) = match inner.find('|') {
            Some(p) => (&inner[..p], Some(&inner[p + 1..])),
            None => (inner, None),
        };

        if target.trim().to_lowercase() == old_lower {
            changed = true;
            out.push_str("[[");
            out.push_str(new_title);
            if let Some(a) = alias {
                out.push('|');
                out.push_str(a);
            }
            out.push_str("]]");
        } else {
            out.push_str("[[");
            out.push_str(inner);
            out.push_str("]]");
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    (out, changed)
}

#[tauri::command]
pub async fn rename_note_links(
    pool: State<'_, SqlitePool>,
    old_title: String,
    new_title: String,
) -> AppResult<i64> {
    rename_note_links_impl(pool.inner(), old_title, new_title).await
}

// Переписывает [[old_title]]/[[old_title|alias]] во всех заметках на new_title.
// Возвращает число обновлённых заметок. Пустой/неизменившийся old_title — no-op
// (переименование "Без названия" → "Без названия" не должно ничего переписывать).
pub async fn rename_note_links_impl(pool: &SqlitePool, old_title: String, new_title: String) -> AppResult<i64> {
    let old_title = old_title.trim();
    let new_title = new_title.trim();
    // eq_ignore_ascii_case не покрывает кириллицу и другой не-ASCII — сравниваем
    // через to_lowercase (Юникодная свёртка регистра).
    if old_title.is_empty() || old_title.to_lowercase() == new_title.to_lowercase() {
        return Ok(0);
    }

    let rows = sqlx::query("SELECT id, content FROM notes")
        .fetch_all(pool)
        .await?;

    let mut updated = 0i64;
    let now = Utc::now().to_rfc3339();
    for row in rows {
        let id: String = row.get("id");
        let content: String = row.get("content");
        let (new_content, changed) = rewrite_links(&content, old_title, new_title);
        if changed {
            sqlx::query("UPDATE notes SET content = ?, updated_at = ? WHERE id = ?")
                .bind(&new_content)
                .bind(&now)
                .bind(&id)
                .execute(pool)
                .await?;
            updated += 1;
        }
    }
    Ok(updated)
}

#[tauri::command]
pub async fn search_notes(pool: State<'_, SqlitePool>, query: String) -> AppResult<Vec<Note>> {
    search_notes_impl(pool.inner(), query).await
}

pub async fn search_notes_impl(pool: &SqlitePool, query: String) -> AppResult<Vec<Note>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // Как в search_tasks: сырой ввод — не синтаксис FTS5, оборачиваем в
    // quoted-phrase-prefix, кавычки удваиваем.
    let escaped = trimmed.replace('"', "\"\"");
    let fts_query = format!("\"{}\"*", escaped);

    let rows = sqlx::query(
        "SELECT n.id, n.title, n.content, n.tags, n.linked_task_id, n.project_id, n.created_at, n.updated_at
         FROM notes n
         INNER JOIN notes_fts ON notes_fts.rowid = n.rowid
         WHERE notes_fts MATCH ?
         ORDER BY rank"
    )
    .bind(fts_query)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_note).collect())
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
    async fn fts_search_finds_and_stays_in_sync() {
        let pool = test_pool().await;

        let note = create_note_impl(&pool, CreateNote {
            title: "Рецепт борща".into(),
            content: "свёкла, капуста".into(),
            tags: vec!["еда".into()],
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();

        // По заголовку, по содержимому, по тегу; префиксно.
        assert_eq!(search_notes_impl(&pool, "борщ".into()).await.unwrap().len(), 1);
        assert_eq!(search_notes_impl(&pool, "капуст".into()).await.unwrap().len(), 1);
        assert_eq!(search_notes_impl(&pool, "еда".into()).await.unwrap().len(), 1);
        assert!(search_notes_impl(&pool, "плов".into()).await.unwrap().is_empty());

        // Спецсимволы FTS5 в запросе не роняют MATCH.
        assert!(search_notes_impl(&pool, "борщ-2 \"AND (x:y)".into()).await.unwrap().is_empty());
        assert!(search_notes_impl(&pool, "   ".into()).await.unwrap().is_empty());

        // После UPDATE индекс видит новый текст и не видит старый.
        update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None,
            content: Some("теперь про плов".into()),
            tags: None,
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();
        assert_eq!(search_notes_impl(&pool, "плов".into()).await.unwrap().len(), 1);
        assert!(search_notes_impl(&pool, "капуст".into()).await.unwrap().is_empty());

        // После DELETE ничего не находится.
        delete_note_impl(&pool, note.id).await.unwrap();
        assert!(search_notes_impl(&pool, "плов".into()).await.unwrap().is_empty());
        assert!(search_notes_impl(&pool, "борщ".into()).await.unwrap().is_empty());
    }

    #[test]
    fn rewrite_links_covers_alias_case_and_self_link() {
        // Простая ссылка
        let (out, changed) = rewrite_links("см. [[Идея]] тут", "Идея", "Новая идея");
        assert_eq!(out, "см. [[Новая идея]] тут");
        assert!(changed);

        // Алиас сохраняется, меняется только цель
        let (out, changed) = rewrite_links("[[Идея|вот тут]]", "Идея", "Новая идея");
        assert_eq!(out, "[[Новая идея|вот тут]]");
        assert!(changed);

        // Регистронезависимо
        let (out, changed) = rewrite_links("[[идея]]", "Идея", "Новая идея");
        assert_eq!(out, "[[Новая идея]]");
        assert!(changed);

        // Несколько ссылок, только совпадающие переписываются
        let (out, changed) = rewrite_links("[[Идея]] и [[Другая]] и снова [[Идея|та же]]", "Идея", "X");
        assert_eq!(out, "[[X]] и [[Другая]] и снова [[X|та же]]");
        assert!(changed);

        // Без совпадений — не менялось
        let (out, changed) = rewrite_links("[[Другая]] заметка", "Идея", "X");
        assert_eq!(out, "[[Другая]] заметка");
        assert!(!changed);

        // Самоссылка [[Идея]] → [[X]] переписывается как обычная ссылка
        let (out, changed) = rewrite_links("это [[Идея]] сама на себя", "Идея", "X");
        assert_eq!(out, "это [[X]] сама на себя");
        assert!(changed);

        // Незакрытая ссылка не роняет парсинг
        let (out, changed) = rewrite_links("текст [[Идея без закрытия", "Идея", "X");
        assert_eq!(out, "текст [[Идея без закрытия");
        assert!(!changed);
    }

    #[tokio::test]
    async fn rename_note_links_updates_across_notes_and_counts() {
        let pool = test_pool().await;

        let target = create_note_impl(&pool, CreateNote {
            title: "Идея".into(), content: "исходная".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();
        let referrer1 = create_note_impl(&pool, CreateNote {
            title: "Черновик".into(), content: "см. [[Идея]]".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();
        let referrer2 = create_note_impl(&pool, CreateNote {
            title: "Заметки".into(), content: "[[идея|та самая]] и [[Другая]]".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();
        let unrelated = create_note_impl(&pool, CreateNote {
            title: "Не связана".into(), content: "просто текст".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        let count = rename_note_links_impl(&pool, "Идея".into(), "Идея v2".into()).await.unwrap();
        assert_eq!(count, 2); // referrer1 и referrer2; target и unrelated не считаются

        let all = get_notes_impl(&pool).await.unwrap();
        let by_id = |id: &str| all.iter().find(|n| n.id == id).unwrap().content.clone();
        assert_eq!(by_id(&referrer1.id), "см. [[Идея v2]]");
        assert_eq!(by_id(&referrer2.id), "[[Идея v2|та самая]] и [[Другая]]");
        assert_eq!(by_id(&unrelated.id), "просто текст");
        assert_eq!(by_id(&target.id), "исходная"); // содержимое цели не переписывается

        // Пустой old_title и неизменившийся регистр — no-op
        assert_eq!(rename_note_links_impl(&pool, "".into(), "X".into()).await.unwrap(), 0);
        assert_eq!(rename_note_links_impl(&pool, "Идея v2".into(), "идея v2".into()).await.unwrap(), 0);
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
