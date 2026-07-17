// ИИ-автолинковка заметок (v0.6.8): «Предложить связи» — модель смотрит на
// текст текущей заметки и список названий остальных, предлагает, на какие
// стоит сослаться. Тот же принцип, что у планировщика (commands::planner):
// ответ — строгий JSON, но модели не доверяем — сами вырезаем массив и
// фильтруем по реальным названиям заметок.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{Emitter, Manager};
use crate::commands::ai::ask_ai;

const SYSTEM_LINKS: &str = "Ты помогаешь связывать заметки в вики. Дан текст заметки и список \
названий других существующих заметок. Выбери до 5 заголовков из списка, которые тематически \
связаны с текстом (только из списка, не придумывай новые). \
Ответь ТОЛЬКО JSON-массивом строк-заголовков, без пояснений. Если связей нет — верни [].";

#[derive(Deserialize)]
struct RawLinks(Vec<String>);

#[derive(Clone, Serialize)]
pub struct LinkSuggestPayload {
    pub note_id: String,
    pub titles: Vec<String>,
    pub error: Option<String>,
}

// Модель отвечает произвольным текстом вокруг JSON — вырезаем массив, парсим
// снисходительно, но каждый заголовок обязан совпасть (без учёта регистра) с
// одним из реально существующих — иначе выдумка модели тихо отбрасывается.
pub fn parse_link_suggestions(raw: &str, known_titles: &[String]) -> Vec<String> {
    let Some(s) = raw.find('[') else { return vec![] };
    let Some(e) = raw.rfind(']') else { return vec![] };
    if s >= e { return vec![]; }
    let Ok(RawLinks(items)) = serde_json::from_str(&raw[s..=e]) else { return vec![] };

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let key = item.trim().to_lowercase();
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if let Some(real) = known_titles.iter().find(|t| t.to_lowercase() == key) {
            out.push(real.clone());
            if out.len() >= 5 {
                break;
            }
        }
    }
    out
}

pub async fn build_prompt(pool: &SqlitePool, note_id: &str) -> Result<(String, Vec<String>), String> {
    let note = crate::commands::notes::get_notes_impl(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|n| n.id == note_id)
        .ok_or_else(|| "Заметка не найдена".to_string())?;

    let others: Vec<String> = crate::commands::notes::get_notes_impl(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|n| n.id != note_id)
        .map(|n| n.title)
        .collect();

    if others.is_empty() {
        return Err("Больше нет заметок, с которыми можно связать эту".into());
    }

    let prompt = format!(
        "Текст заметки «{}»:\n{}\n\nСписок других заметок:\n{}",
        note.title,
        note.content.chars().take(2000).collect::<String>(),
        others.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n"),
    );
    Ok((prompt, others))
}

#[tauri::command]
pub async fn ai_suggest_links(app: tauri::AppHandle, note_id: String) -> Result<(), String> {
    tokio::spawn(async move {
        let id = note_id.clone();
        let r: Result<Vec<String>, String> = async {
            let pool = app.state::<SqlitePool>();
            let (prompt, others) = build_prompt(pool.inner(), &note_id).await?;
            let raw = ask_ai(&app, SYSTEM_LINKS, &prompt).await?;
            Ok(parse_link_suggestions(&raw, &others))
        }.await;

        let payload = match r {
            Ok(titles) => LinkSuggestPayload { note_id: id, titles, error: None },
            Err(e) => LinkSuggestPayload { note_id: id, titles: vec![], error: Some(e) },
        };
        let _ = app.emit("ai-links", payload);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Vec<String> {
        vec!["Идея A".into(), "Заметка Б".into(), "Проект В".into()]
    }

    #[test]
    fn parses_valid_titles_and_dedupes() {
        let raw = r#"Вот связи: ["Идея A", "идея a", "Проект В"] — готово"#;
        assert_eq!(parse_link_suggestions(raw, &known()), vec!["Идея A", "Проект В"]);
    }

    #[test]
    fn drops_titles_not_in_known_list() {
        let raw = r#"["Идея A", "Выдуманная заметка", "Ещё одна выдумка"]"#;
        assert_eq!(parse_link_suggestions(raw, &known()), vec!["Идея A"]);
    }

    #[test]
    fn empty_array_and_garbage_yield_empty() {
        assert_eq!(parse_link_suggestions("[]", &known()), Vec::<String>::new());
        assert_eq!(parse_link_suggestions("не json вообще", &known()), Vec::<String>::new());
        assert_eq!(parse_link_suggestions("", &known()), Vec::<String>::new());
    }

    #[test]
    fn caps_at_five_suggestions() {
        let known: Vec<String> = (1..=10).map(|i| format!("Заметка {i}")).collect();
        let raw = serde_json::to_string(&known).unwrap();
        assert_eq!(parse_link_suggestions(&raw, &known).len(), 5);
    }

    #[tokio::test]
    async fn build_prompt_excludes_self_and_errors_when_alone() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();

        let note = crate::commands::notes::create_note_impl(&pool, crate::commands::notes::CreateNote {
            title: "Одинокая".into(), content: "текст".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        assert!(build_prompt(&pool, &note.id).await.is_err());

        let other = crate::commands::notes::create_note_impl(&pool, crate::commands::notes::CreateNote {
            title: "Соседняя".into(), content: "x".into(),
            tags: vec![], linked_task_id: None, project_id: None,
        }).await.unwrap();

        let (prompt, others) = build_prompt(&pool, &note.id).await.unwrap();
        assert_eq!(others, vec!["Соседняя"]);
        assert!(prompt.contains("Одинокая"));
        assert!(prompt.contains("Соседняя"));
        assert!(!prompt.contains(&other.id)); // id не должен утекать в промпт
    }
}
