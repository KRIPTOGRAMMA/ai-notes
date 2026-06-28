use tauri::{Emitter, Manager};
use serde::Serialize;
use crate::ai::sidecar::{SharedSidecar, ensure_running};
use crate::ai::engine::ask;

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
    // Try JSON array first.
    let trimmed = raw.trim();
    let json_start = trimmed.find('[').unwrap_or(0);
    let json_end = trimmed.rfind(']').map(|i| i + 1).unwrap_or(trimmed.len());
    if let Ok(items) = serde_json::from_str::<Vec<String>>(&trimmed[json_start..json_end]) {
        if !items.is_empty() {
            return Some(items.join("|||"));
        }
    }

    // Fallback: parse numbered list lines like "1. Do something"
    let items: Vec<String> = trimmed
        .lines()
        .filter_map(|line| {
            let l = line.trim();
            // Strip "1." / "1)" / "-" / "*" prefixes
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

#[tauri::command]
pub async fn ai_rewrite(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let sidecar = app.state::<SharedSidecar>();
            let port = ensure_running(&app, &sidecar).await?;
            ask(port, SYSTEM_REWRITE, &title).await
        }.await;
        let _ = app.emit("ai-result", into_payload(task_id, "rewrite", r));
    });
    Ok(())
}

#[tauri::command]
pub async fn ai_subtasks(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let sidecar = app.state::<SharedSidecar>();
            let port = ensure_running(&app, &sidecar).await?;
            let raw = ask(port, SYSTEM_SUBTASKS, &title).await?;
            parse_subtasks(&raw).ok_or_else(|| format!("Не удалось разобрать ответ модели: {}", raw))
        }.await;
        let _ = app.emit("ai-result", into_payload(task_id, "subtasks", r));
    });
    Ok(())
}

#[tauri::command]
pub async fn ai_classify(app: tauri::AppHandle, task_id: String, title: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let sidecar = app.state::<SharedSidecar>();
            let port = ensure_running(&app, &sidecar).await?;
            ask(port, SYSTEM_CLASSIFY, &title).await
        }.await;
        let _ = app.emit("ai-result", into_payload(task_id, "classify", r));
    });
    Ok(())
}
