use tauri::Emitter;
use sqlx::SqlitePool;
use tauri::Manager;
use serde::Serialize;
use crate::ai::sidecar::{SharedSidecar, ensure_running};
use crate::ai::engine::ask;
use crate::ai::cloud::{ask_openai, ask_anthropic};

const SYSTEM_REWRITE: &str =
    "Перепиши задачу в SMART-формат: чёткая цель, измеримый результат, срок. Только результат, без пояснений.";

const SYSTEM_SUBTASKS: &str =
    "You are a task planner. Split the task into 3-7 subtasks. Reply ONLY with a JSON array of strings, nothing else. Example: [\"subtask 1\", \"subtask 2\", \"subtask 3\"]";

const SYSTEM_CLASSIFY: &str =
    "Категория задачи: Work/Study/Home/Health/Other. Ответь одним словом.";

#[derive(Clone, Serialize)]
pub struct AiResult {
    pub task_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

fn parse_subtasks(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let json_start = trimmed.find('[').unwrap_or(0);
    let json_end = trimmed.rfind(']').map(|i| i + 1).unwrap_or(trimmed.len());
    if let Ok(items) = serde_json::from_str::<Vec<String>>(&trimmed[json_start..json_end]) {
        if !items.is_empty() {
            return Some(items.join("|||"));
        }
    }

    let items: Vec<String> = trimmed
        .lines()
        .filter_map(|line| {
            let l = line.trim();
            let stripped = l
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim_start_matches(['.', ')', '-', '*', ' '])
                .trim();
            if stripped.is_empty() { None } else { Some(stripped.to_string()) }
        })
        .collect();

    if items.is_empty() { None } else { Some(items.join("|||")) }
}

fn into_payload(task_id: String, kind: &str, r: Result<String, String>) -> AiResult {
    let (result, error) = match r { Ok(v) => (Some(v), None), Err(e) => (None, Some(e)) };
    AiResult { task_id, kind: kind.into(), result, error }
}

async fn ask_ai(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    let settings = crate::commands::settings::load_settings_raw(app.state::<SqlitePool>().inner())
        .await
        .map_err(|e| e.to_string())?;

    match settings.ai_provider.as_str() {
        "openai" if !settings.openai_key.is_empty() => {
            ask_openai(&settings.openai_key, &settings.openai_model, system, user).await
        }
        "anthropic" if !settings.anthropic_key.is_empty() => {
            ask_anthropic(&settings.anthropic_key, &settings.anthropic_model, system, user).await
        }
        _ => {
            let sidecar = app.state::<SharedSidecar>();
            let port = ensure_running(app, &sidecar).await?;
            ask(port, system, user).await
        }
    }
}

#[tauri::command]
pub async fn ai_rewrite(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = ask_ai(&app, SYSTEM_REWRITE, &title).await;
        let _ = app.emit("ai-result", into_payload(task_id, "rewrite", r));
    });
    Ok(())
}

#[tauri::command]
pub async fn ai_subtasks(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let raw = ask_ai(&app, SYSTEM_SUBTASKS, &title).await?;
            parse_subtasks(&raw).ok_or_else(|| format!("Не удалось разобрать ответ модели: {}", raw))
        }.await;
        let _ = app.emit("ai-result", into_payload(task_id, "subtasks", r));
    });
    Ok(())
}

#[tauri::command]
pub async fn ai_classify(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = ask_ai(&app, SYSTEM_CLASSIFY, &title).await;
        let _ = app.emit("ai-result", into_payload(task_id, "classify", r));
    });
    Ok(())
}
