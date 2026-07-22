use tauri::State;
use sqlx::{SqlitePool, Row};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub linked_task_id: Option<String>,
    pub project_id: Option<String>,
    pub pinned: bool,
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
    pub pinned: Option<bool>,
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
    let pinned: i64 = row.get("pinned");
    Note {
        id: row.get("id"),
        title: row.get("title"),
        content: row.get("content"),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        linked_task_id: row.get("linked_task_id"),
        project_id: row.get("project_id"),
        pinned: pinned != 0,
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
        "SELECT id, title, content, tags, linked_task_id, project_id, pinned, created_at, updated_at FROM notes ORDER BY updated_at DESC"
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
        pinned: false,
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
        snapshot_revision_if_due(pool, &id, &now).await?;
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
    if let Some(pinned) = patch.pinned {
        sqlx::query("UPDATE notes SET pinned = ?, updated_at = ? WHERE id = ?")
            .bind(pinned).bind(&now).bind(&id)
            .execute(pool).await?;
    }

    // fetch_optional, не fetch_one: заметка могла быть удалена параллельно
    // (автосейв debounced на 800мс — пользователь успевает нажать «Удалить»
    // раньше, чем сработает предыдущий таймер сохранения). Это не ошибка
    // сохранения — заметки уже нет, апдейт стал no-op, откатывать нечего.
    let row = sqlx::query(
        "SELECT id, title, content, tags, linked_task_id, project_id, pinned, created_at, updated_at FROM notes WHERE id = ?"
    )
    .bind(&id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Other("__NOTE_DELETED__".into()))?;

    Ok(row_to_note(row))
}

const REVISION_INTERVAL_MINS: i64 = 10;
const REVISION_KEEP: i64 = 20;

// Снимок ДО записи нового content: только если последняя ревизия этой заметки
// старше REVISION_INTERVAL_MINS (или ревизий ещё нет вовсе — первая правка тоже
// снимается, чтобы можно было откатиться к исходному тексту). Не снимает
// снимок, если content ещё не менялся в БД (частые автосейвы одного и того же
// текста не плодят ревизии) — сравниваем с текущим content заметки.
async fn snapshot_revision_if_due(pool: &SqlitePool, note_id: &str, now: &str) -> AppResult<()> {
    let current: Option<String> = sqlx::query_scalar("SELECT content FROM notes WHERE id = ?")
        .bind(note_id)
        .fetch_optional(pool)
        .await?;
    let Some(current) = current else { return Ok(()) };

    let last_at: Option<String> = sqlx::query_scalar(
        "SELECT created_at FROM note_revisions WHERE note_id = ? ORDER BY created_at DESC LIMIT 1"
    )
    .bind(note_id)
    .fetch_optional(pool)
    .await?;

    let due = match &last_at {
        None => true,
        Some(last) => {
            let now_dt = chrono::DateTime::parse_from_rfc3339(now).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now());
            let last_dt = chrono::DateTime::parse_from_rfc3339(last).map(|d| d.with_timezone(&Utc));
            match last_dt {
                Ok(last_dt) => (now_dt - last_dt).num_minutes() >= REVISION_INTERVAL_MINS,
                Err(_) => true,
            }
        }
    };
    if !due {
        return Ok(());
    }

    sqlx::query("INSERT INTO note_revisions (id, note_id, content, created_at) VALUES (?, ?, ?, ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(note_id)
        .bind(&current)
        .bind(now)
        .execute(pool)
        .await?;

    rotate_revisions(pool, note_id).await
}

// Держим ≤ REVISION_KEEP ревизий на заметку — старейшие лишние удаляем.
async fn rotate_revisions(pool: &SqlitePool, note_id: &str) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM note_revisions WHERE note_id = ? AND id NOT IN (
            SELECT id FROM note_revisions WHERE note_id = ? ORDER BY created_at DESC LIMIT ?
        )"
    )
    .bind(note_id)
    .bind(note_id)
    .bind(REVISION_KEEP)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
pub struct NoteRevision {
    pub id: String,
    pub created_at: String,
    pub size: i64,
}

#[tauri::command]
pub async fn get_note_revisions(pool: State<'_, SqlitePool>, note_id: String) -> AppResult<Vec<NoteRevision>> {
    get_note_revisions_impl(pool.inner(), &note_id).await
}

pub async fn get_note_revisions_impl(pool: &SqlitePool, note_id: &str) -> AppResult<Vec<NoteRevision>> {
    let rows = sqlx::query(
        "SELECT id, created_at, length(content) as size FROM note_revisions WHERE note_id = ? ORDER BY created_at DESC"
    )
    .bind(note_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| NoteRevision {
        id: r.get("id"),
        created_at: r.get("created_at"),
        size: r.get("size"),
    }).collect())
}

#[tauri::command]
pub async fn get_note_revision_content(pool: State<'_, SqlitePool>, revision_id: String) -> AppResult<String> {
    get_note_revision_content_impl(pool.inner(), &revision_id).await
}

pub async fn get_note_revision_content_impl(pool: &SqlitePool, revision_id: &str) -> AppResult<String> {
    sqlx::query_scalar("SELECT content FROM note_revisions WHERE id = ?")
        .bind(revision_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| crate::error::AppError::Other("Ревизия не найдена".into()))
}

// Откат к ревизии: текущее содержимое заметки тоже сохраняется ревизией
// (иначе несохранённая на момент отката правка теряется без следа), затем
// content заметки заменяется содержимым выбранной ревизии.
#[tauri::command]
pub async fn restore_note_revision(pool: State<'_, SqlitePool>, revision_id: String) -> AppResult<Note> {
    restore_note_revision_impl(pool.inner(), &revision_id).await
}

pub async fn restore_note_revision_impl(pool: &SqlitePool, revision_id: &str) -> AppResult<Note> {
    let row = sqlx::query("SELECT note_id, content FROM note_revisions WHERE id = ?")
        .bind(revision_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| crate::error::AppError::Other("Ревизия не найдена".into()))?;
    let note_id: String = row.get("note_id");
    let revision_content: String = row.get("content");

    let now = Utc::now().to_rfc3339();
    // Текущее содержимое — тоже в ревизию, без учёта 10-минутного интервала
    // (откат — явное действие пользователя, а не автосейв).
    let current: Option<String> = sqlx::query_scalar("SELECT content FROM notes WHERE id = ?")
        .bind(&note_id)
        .fetch_optional(pool)
        .await?;
    if let Some(current) = current {
        sqlx::query("INSERT INTO note_revisions (id, note_id, content, created_at) VALUES (?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string())
            .bind(&note_id)
            .bind(&current)
            .bind(&now)
            .execute(pool)
            .await?;
        rotate_revisions(pool, &note_id).await?;
    }

    sqlx::query("UPDATE notes SET content = ?, updated_at = ? WHERE id = ?")
        .bind(&revision_content)
        .bind(&now)
        .bind(&note_id)
        .execute(pool)
        .await?;

    let row = sqlx::query(
        "SELECT id, title, content, tags, linked_task_id, project_id, pinned, created_at, updated_at FROM notes WHERE id = ?"
    )
    .bind(&note_id)
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
        "SELECT n.id, n.title, n.content, n.tags, n.linked_task_id, n.project_id, n.pinned, n.created_at, n.updated_at
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

#[derive(Debug, Serialize, Clone)]
pub struct NoteSnippet {
    pub item: Note,
    pub snippet: String,
}

#[tauri::command]
pub async fn search_notes_snippet(pool: State<'_, SqlitePool>, query: String) -> AppResult<Vec<NoteSnippet>> {
    search_notes_snippet_impl(pool.inner(), query).await
}

pub async fn search_notes_snippet_impl(pool: &SqlitePool, query: String) -> AppResult<Vec<NoteSnippet>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    let escaped = trimmed.replace('"', "\"\"");
    let fts_query = format!("\"{}\"*", escaped);

    let rows = sqlx::query(
        "SELECT n.id, n.title, n.content, n.tags, n.linked_task_id, n.project_id, n.pinned, n.created_at, n.updated_at,
                snippet(notes_fts, 1, '<mark>', '</mark>', '…', 32) AS snippet
         FROM notes n
         INNER JOIN notes_fts ON notes_fts.rowid = n.rowid
         WHERE notes_fts MATCH ?
         ORDER BY rank"
    )
    .bind(fts_query)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| {
        let snippet: Option<String> = r.get("snippet");
        NoteSnippet { item: row_to_note(r), snippet: snippet.unwrap_or_default() }
    }).collect())
}

#[tauri::command]
pub async fn delete_note(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_note_impl(pool.inner(), id).await
}

pub async fn delete_note_impl(pool: &SqlitePool, id: String) -> AppResult<()> {
    sqlx::query("DELETE FROM note_revisions WHERE note_id = ?")
        .bind(&id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(())
}

// Символы, недопустимые/проблемные в именах файлов на распространённых ФС
// (Windows тоже, т.к. экспорт может быть скопирован туда) — заменяем на "_".
fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .trim()
        .chars()
        .map(|c| if r#"/\:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c })
        .collect();
    if cleaned.is_empty() { "Без названия".to_string() } else { cleaned }
}

#[tauri::command]
pub async fn export_notes_md(pool: State<'_, SqlitePool>, dir: String) -> AppResult<usize> {
    export_notes_md_impl(pool.inner(), std::path::Path::new(&dir)).await
}

// Каждая заметка → <санитизированное имя>.md, контент как есть (вики-ссылки
// уже совместимы с Obsidian). Коллизии имён (после санитайза, включая регистр
// разных заметок с одинаковым названием) — суффикс "-2", "-3"... по порядку.
pub async fn export_notes_md_impl(pool: &SqlitePool, dir: &std::path::Path) -> AppResult<usize> {
    let notes = get_notes_impl(pool).await?;
    std::fs::create_dir_all(dir)?;

    let mut used: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut count = 0usize;
    for note in &notes {
        let base = sanitize_filename(&note.title);
        let key = base.to_lowercase();
        let n = used.entry(key).or_insert(0);
        *n += 1;
        let filename = if *n == 1 { format!("{base}.md") } else { format!("{base}-{n}.md") };
        std::fs::write(dir.join(&filename), &note.content)?;
        count += 1;
    }
    Ok(count)
}

// Экспорт одной заметки в самодостаточный HTML-файл (v0.9.08). Рендер
// markdown → HTML и встраивание картинок как data: URI делает фронт
// (renderMarkdown + DOMPurify уже там), команда лишь пишет готовую строку
// на диск — как export_notes_md, но без обращения к БД.
#[tauri::command]
pub fn export_note_html(path: String, html: String) -> AppResult<()> {
    std::fs::write(&path, html)?;
    Ok(())
}

#[tauri::command]
pub async fn import_notes_md(pool: State<'_, SqlitePool>, dir: String) -> AppResult<usize> {
    import_notes_md_impl(pool.inner(), std::path::Path::new(&dir)).await
}

// Все *.md в папке (не рекурсивно) → новые заметки: title = имя файла без
// расширения, content = содержимое как есть. Совпадение с уже существующим
// названием НЕ мёржится — создаётся отдельная новая заметка (пользователь сам
// разберётся через дубликаты; тише через тихий merge было бы неожиданнее).
pub async fn import_notes_md_impl(pool: &SqlitePool, dir: &std::path::Path) -> AppResult<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Без названия").to_string();
        let content = std::fs::read_to_string(&path)?;
        create_note_impl(pool, CreateNote {
            title,
            content,
            tags: vec![],
            linked_task_id: None,
            project_id: None,
        }).await?;
        count += 1;
    }
    Ok(count)
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
            pinned: None,
        }).await.unwrap();
        assert_eq!(updated.content, "новый текст");
        assert_eq!(updated.title, "заметка"); // не тронут

        delete_note_impl(&pool, note.id).await.unwrap();
        assert!(get_notes_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn pinned_defaults_false_and_toggles_via_update() {
        let pool = test_pool().await;

        let note = create_note_impl(&pool, CreateNote {
            title: "закрепи меня".into(),
            content: "текст".into(),
            tags: vec![],
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();
        assert!(!note.pinned);

        let pinned = update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None, content: None, tags: None, linked_task_id: None, project_id: None,
            pinned: Some(true),
        }).await.unwrap();
        assert!(pinned.pinned);

        // Переживает перечитывание из БД (не только in-memory возврат update).
        let all = get_notes_impl(&pool).await.unwrap();
        assert!(all.iter().find(|n| n.id == note.id).unwrap().pinned);

        let unpinned = update_note_impl(&pool, note.id.clone(), UpdateNote {
            title: None, content: None, tags: None, linked_task_id: None, project_id: None,
            pinned: Some(false),
        }).await.unwrap();
        assert!(!unpinned.pinned);
    }

    // Гонка автосейва с удалением: debounced-сохранение может долететь до
    // бэкенда уже после того, как пользователь удалил заметку. UPDATE на
    // отсутствующую строку — безвредный no-op, но раньше finalизирующий
    // SELECT падал с RowNotFound и всплывал как видимая ошибка, хотя удаление
    // уже реально прошло — чинили именно это.
    #[tokio::test]
    async fn update_after_delete_is_soft_error_not_panic() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "v1".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        delete_note_impl(&pool, note.id.clone()).await.unwrap();

        let r = update_note_impl(&pool, note.id.clone(), content_patch("v2")).await;
        assert!(r.is_err());
        // Заметка остаётся удалённой — UPDATE-гонка не воскрешает и не дублирует её.
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
            pinned: None,
        }).await.unwrap();
        assert_eq!(search_notes_impl(&pool, "плов".into()).await.unwrap().len(), 1);
        assert!(search_notes_impl(&pool, "капуст".into()).await.unwrap().is_empty());

        // После DELETE ничего не находится.
        delete_note_impl(&pool, note.id).await.unwrap();
        assert!(search_notes_impl(&pool, "плов".into()).await.unwrap().is_empty());
        assert!(search_notes_impl(&pool, "борщ".into()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fts_snippet_returns_markers() {
        let pool = test_pool().await;

        create_note_impl(&pool, CreateNote {
            title: "рецепт".into(),
            content: "свёкла и капуста для борща".into(),
            tags: vec!["еда".into()],
            linked_task_id: None,
            project_id: None,
        }).await.unwrap();

        let results = search_notes_snippet_impl(&pool, "капуста".into()).await.unwrap();
        assert_eq!(results.len(), 1, "snippet={:?}", results[0].snippet);
        assert!(results[0].snippet.contains("<mark>"), "snippet should contain <mark>, got: {:?}", results[0].snippet);
        assert!(results[0].snippet.contains("</mark>"), "snippet should contain </mark>");
        assert!(results[0].snippet.contains("капуста"), "snippet should contain query word");
        assert_eq!(results[0].item.content, "свёкла и капуста для борща");
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
            pinned: None,
        }).await.unwrap();
        assert_eq!(updated.tags, vec!["done"]);
        assert_eq!(updated.linked_task_id, None);
        assert_eq!(updated.project_id, None);
    }

    fn content_patch(content: &str) -> UpdateNote {
        UpdateNote { title: None, content: Some(content.into()), tags: None, linked_task_id: None, project_id: None, pinned: None }
    }

    async fn revision_count(pool: &SqlitePool, note_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM note_revisions WHERE note_id = ?")
            .bind(note_id).fetch_one(pool).await.unwrap()
    }

    async fn set_last_revision_at(pool: &SqlitePool, note_id: &str, at: &str) {
        sqlx::query("UPDATE note_revisions SET created_at = ? WHERE note_id = ? AND id = (
            SELECT id FROM note_revisions WHERE note_id = ? ORDER BY created_at DESC LIMIT 1
        )")
        .bind(at).bind(note_id).bind(note_id)
        .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn first_content_edit_snapshots_original() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "исходный текст".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        update_note_impl(&pool, note.id.clone(), content_patch("новый текст")).await.unwrap();

        assert_eq!(revision_count(&pool, &note.id).await, 1);
        let revs = get_note_revisions_impl(&pool, &note.id).await.unwrap();
        assert_eq!(revs.len(), 1);
    }

    #[tokio::test]
    async fn second_edit_within_interval_does_not_snapshot_again() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "v1".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        update_note_impl(&pool, note.id.clone(), content_patch("v2")).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 1);

        // Правка сразу после — интервал (10 мин) ещё не прошёл
        update_note_impl(&pool, note.id.clone(), content_patch("v3")).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 1);
    }

    #[tokio::test]
    async fn edit_after_interval_snapshots_again() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "v1".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        update_note_impl(&pool, note.id.clone(), content_patch("v2")).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 1);

        // Отодвигаем последнюю ревизию на 11 минут назад — интервал прошёл
        let stale = (Utc::now() - chrono::Duration::minutes(11)).to_rfc3339();
        set_last_revision_at(&pool, &note.id, &stale).await;

        update_note_impl(&pool, note.id.clone(), content_patch("v3")).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 2);
    }

    #[tokio::test]
    async fn rotation_keeps_at_most_twenty() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "v0".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        // 25 правок, каждая "старит" предыдущую ревизию за интервал, чтобы снимок случился
        for i in 1..=25 {
            update_note_impl(&pool, note.id.clone(), content_patch(&format!("v{i}"))).await.unwrap();
            let stale = (Utc::now() - chrono::Duration::minutes(11)).to_rfc3339();
            set_last_revision_at(&pool, &note.id, &stale).await;
        }

        assert_eq!(revision_count(&pool, &note.id).await, 20);
    }

    #[tokio::test]
    async fn restore_cycle_swaps_content_and_snapshots_current() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "оригинал".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        update_note_impl(&pool, note.id.clone(), content_patch("изменённый")).await.unwrap();
        let revs = get_note_revisions_impl(&pool, &note.id).await.unwrap();
        assert_eq!(revs.len(), 1);
        let original_rev_id = revs[0].id.clone();

        let restored = restore_note_revision_impl(&pool, &original_rev_id).await.unwrap();
        assert_eq!(restored.content, "оригинал");

        // Текущее ("изменённый") тоже попало в ревизии — можно вернуться вперёд
        let revs_after = get_note_revisions_impl(&pool, &note.id).await.unwrap();
        assert_eq!(revs_after.len(), 2);
    }

    #[tokio::test]
    async fn restore_missing_revision_errors() {
        let pool = test_pool().await;
        let r = restore_note_revision_impl(&pool, "no-such-id").await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn delete_note_cascades_revisions() {
        let pool = test_pool().await;
        let note = create_note_impl(&pool, CreateNote {
            title: "т".into(), content: "v1".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();
        update_note_impl(&pool, note.id.clone(), content_patch("v2")).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 1);

        delete_note_impl(&pool, note.id.clone()).await.unwrap();
        assert_eq!(revision_count(&pool, &note.id).await, 0);
    }

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ai-notes-md-test-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn export_roundtrip_recreates_notes_on_import() {
        let pool = test_pool().await;
        create_note_impl(&pool, CreateNote {
            title: "Первая заметка".into(), content: "текст один".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();
        create_note_impl(&pool, CreateNote {
            title: "Вторая заметка".into(), content: "[[Первая заметка]] текст два".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        let dir = temp_dir();
        let exported = export_notes_md_impl(&pool, &dir).await.unwrap();
        assert_eq!(exported, 2);
        assert!(dir.join("Первая заметка.md").exists());
        assert!(dir.join("Вторая заметка.md").exists());

        let pool2 = test_pool().await;
        let imported = import_notes_md_impl(&pool2, &dir).await.unwrap();
        assert_eq!(imported, 2);
        let notes = get_notes_impl(&pool2).await.unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.title == "Первая заметка" && n.content == "текст один"));
        assert!(notes.iter().any(|n| n.title == "Вторая заметка" && n.content.contains("[[Первая заметка]]")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_filename_replaces_forbidden_chars() {
        assert_eq!(sanitize_filename("отчёт: план/факт"), "отчёт_ план_факт");
        assert_eq!(sanitize_filename("a<b>c|d?e*f\"g"), "a_b_c_d_e_f_g");
        assert_eq!(sanitize_filename("   "), "Без названия");
        assert_eq!(sanitize_filename(""), "Без названия");
    }

    #[tokio::test]
    async fn export_disambiguates_duplicate_titles() {
        let pool = test_pool().await;
        for _ in 0..3 {
            create_note_impl(&pool, CreateNote {
                title: "дубликат".into(), content: "x".into(),
                tags: vec![], linked_task_id: None, project_id: None,
            }).await.unwrap();
        }
        let dir = temp_dir();
        let exported = export_notes_md_impl(&pool, &dir).await.unwrap();
        assert_eq!(exported, 3);
        assert!(dir.join("дубликат.md").exists());
        assert!(dir.join("дубликат-2.md").exists());
        assert!(dir.join("дубликат-3.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn import_ignores_non_md_files() {
        let pool = test_pool().await;
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("заметка.md"), "содержимое").unwrap();
        std::fs::write(dir.join("картинка.png"), "не текст").unwrap();

        let imported = import_notes_md_impl(&pool, &dir).await.unwrap();
        assert_eq!(imported, 1);
        let notes = get_notes_impl(&pool).await.unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "заметка");
        assert_eq!(notes[0].content, "содержимое");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn import_of_missing_or_empty_dir_is_zero() {
        let pool = test_pool().await;
        let dir = temp_dir(); // не создаём — не существует
        assert_eq!(import_notes_md_impl(&pool, &dir).await.unwrap(), 0);

        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(import_notes_md_impl(&pool, &dir).await.unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn import_duplicate_titles_create_separate_notes() {
        let pool = test_pool().await;
        create_note_impl(&pool, CreateNote {
            title: "уже есть".into(), content: "старое".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("уже есть.md"), "новое").unwrap();

        let imported = import_notes_md_impl(&pool, &dir).await.unwrap();
        assert_eq!(imported, 1);
        let notes = get_notes_impl(&pool).await.unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.content == "старое"));
        assert!(notes.iter().any(|n| n.content == "новое"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
