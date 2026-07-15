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

const SYSTEM_INSIGHT: &str =
    "Ты ассистент по продуктивности. Дай 1–3 коротких предложения про продуктивность пользователя, по-русски. Только текст, без пояснений и списков.";

const SYSTEM_SUMMARY: &str =
    "Ты ассистент по продуктивности. Составь краткое резюме периода (3–5 предложений): что сделано, сколько активного времени, прогресс целей (если есть), что требует внимания. По-русски, только текст.";

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Provider {
    Local,
    OpenAi,
    Anthropic,
}

// Порядок обхода провайдеров при автопереключении: от выбранного основного,
// недоступные (нет ключа / нет model.gguf) выкидываются сразу. Чистая функция.
pub fn resolve_provider_order(
    primary: &str,
    local_available: bool,
    has_openai: bool,
    has_anthropic: bool,
) -> Vec<Provider> {
    let candidates = match primary {
        "openai" => [Provider::OpenAi, Provider::Anthropic, Provider::Local],
        "anthropic" => [Provider::Anthropic, Provider::OpenAi, Provider::Local],
        _ => [Provider::Local, Provider::OpenAi, Provider::Anthropic],
    };
    candidates
        .into_iter()
        .filter(|p| match p {
            Provider::Local => local_available,
            Provider::OpenAi => has_openai,
            Provider::Anthropic => has_anthropic,
        })
        .collect()
}

async fn ask_provider(
    app: &tauri::AppHandle,
    settings: &crate::commands::settings::AppSettings,
    provider: Provider,
    system: &str,
    user: &str,
) -> Result<String, String> {
    match provider {
        Provider::OpenAi => {
            ask_openai(&settings.openai_key, &settings.openai_model, system, user).await
        }
        Provider::Anthropic => {
            ask_anthropic(&settings.anthropic_key, &settings.anthropic_model, system, user).await
        }
        Provider::Local => {
            let sidecar = app.state::<SharedSidecar>();
            let port = ensure_running(app, &sidecar).await?;
            ask(port, system, user).await
        }
    }
}

pub async fn ask_ai(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    let settings = crate::commands::settings::load_settings_raw(app.state::<SqlitePool>().inner())
        .await
        .map_err(|e| e.to_string())?;

    // Явно выключенный ИИ: не поднимаем локальную модель и не ходим в облако
    if settings.ai_provider == "none" {
        return Err("ИИ отключён в настройках".into());
    }

    if !settings.ai_fallback {
        // Прежнее поведение: один провайдер, без отката
        return match settings.ai_provider.as_str() {
            "openai" if !settings.openai_key.is_empty() => {
                ask_openai(&settings.openai_key, &settings.openai_model, system, user).await
            }
            "anthropic" if !settings.anthropic_key.is_empty() => {
                ask_anthropic(&settings.anthropic_key, &settings.anthropic_model, system, user).await
            }
            _ => ask_provider(app, &settings, Provider::Local, system, user).await,
        };
    }

    let order = resolve_provider_order(
        &settings.ai_provider,
        crate::commands::model::local_model_available(app),
        !settings.openai_key.is_empty(),
        !settings.anthropic_key.is_empty(),
    );
    if order.is_empty() {
        return Err("ИИ не настроен: нет ни ключей облака, ни локальной модели".into());
    }

    let mut last_err = String::new();
    for provider in order {
        match ask_provider(app, &settings, provider, system, user).await {
            Ok(v) => return Ok(v),
            Err(e) => last_err = e,
        }
    }
    Err(format!("Все ИИ-провайдеры недоступны. Последняя ошибка: {}", last_err))
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

#[derive(Clone, Serialize)]
pub struct InsightPayload {
    pub result: Option<String>,
    pub error: Option<String>,
}

// Краткая сводка активности за последние дни — вход для ИИ-инсайта.
async fn insight_summary(pool: &SqlitePool) -> Result<String, String> {
    use crate::commands::monitor::{
        get_activity_by_day_impl, get_category_distribution_impl, get_task_completions_by_day_impl,
    };

    let days = get_activity_by_day_impl(pool).await.map_err(|e| e.to_string())?;
    let completions = get_task_completions_by_day_impl(pool).await.map_err(|e| e.to_string())?;
    let cats = get_category_distribution_impl(pool).await.map_err(|e| e.to_string())?;

    let minutes: Vec<String> = days
        .iter()
        .rev()
        .take(7)
        .rev()
        .map(|d| format!("{}: {} мин", d.date, d.minutes))
        .collect();
    let done_recent: i64 = completions.iter().rev().take(7).map(|c| c.completed).sum();
    let top_cat = cats
        .iter()
        .max_by_key(|c| c.count)
        .map(|c| c.category.clone())
        .unwrap_or_else(|| "нет данных".into());

    Ok(format!(
        "Активные минуты по дням: {}. Выполнено задач за последние дни: {}. Топ-категория выполненных задач: {}.",
        if minutes.is_empty() { "нет данных".into() } else { minutes.join(", ") },
        done_recent,
        top_cat
    ))
}

#[tauri::command]
pub async fn dashboard_insight(app: tauri::AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let summary = insight_summary(app.state::<SqlitePool>().inner()).await?;
            ask_ai(&app, SYSTEM_INSIGHT, &summary).await
        }
        .await;
        let (result, error) = match r {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        let _ = app.emit("dashboard-insight", InsightPayload { result, error });
    });
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct SummaryPayload {
    pub kind: String, // "day" | "week"
    pub result: Option<String>,
    pub error: Option<String>,
}

// Данные за период для резюме: выполненные задачи, активные минуты, просрочки.
async fn period_summary(pool: &SqlitePool, days: i64, label: &str) -> Result<String, String> {
    use sqlx::Row;
    let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();

    let done: Vec<String> = sqlx::query(
        "SELECT title FROM tasks WHERE completed_at IS NOT NULL AND completed_at >= ? ORDER BY completed_at",
    )
    .bind(&since)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .iter()
    .map(|r| r.get::<String, _>("title"))
    .collect();

    let active_mins: i64 = sqlx::query(
        "SELECT COALESCE(SUM(duration_secs), 0) / 60 as m FROM activity_log
         WHERE state = 'Active' AND timestamp >= ?",
    )
    .bind(&since)
    .fetch_one(pool)
    .await
    .map(|r| r.get("m"))
    .unwrap_or(0);

    let overdue = crate::notifier::triggers::overdue_count(pool, &chrono::Utc::now().to_rfc3339()).await;

    let mut summary = format!(
        "Период: {}. Выполнено задач: {}{}. Активное время: {} мин. Просрочено сейчас: {}.",
        label,
        done.len(),
        if done.is_empty() { String::new() } else { format!(" ({})", done.join(", ")) },
        active_mins,
        overdue
    );

    // Недельное ревью (v0.5.6): цели проектов и топ приложений — данные фаз 1–2
    if days >= 7 {
        let projects = crate::commands::projects::get_projects_impl(pool).await.unwrap_or_default();
        let goals: Vec<String> = projects
            .iter()
            .filter(|p| !p.archived && (p.goal_tasks.is_some() || p.goal_mins.is_some()))
            .map(|p| {
                let mut parts = vec![];
                if let Some(n) = p.goal_tasks { parts.push(format!("{}/{} задач", p.goal_done_tasks, n)); }
                if let Some(n) = p.goal_mins { parts.push(format!("{}/{} мин", p.goal_done_mins, n)); }
                format!("{}: {}", p.name, parts.join(", "))
            })
            .collect();
        if !goals.is_empty() {
            summary.push_str(&format!(" Цели проектов: {}.", goals.join("; ")));
        }

        let apps = crate::commands::monitor::get_app_usage_impl(pool, 7).await.unwrap_or_default();
        let top: Vec<String> = apps.iter().take(3)
            .map(|a| format!("{} ({} мин)", a.app, a.minutes))
            .collect();
        if !top.is_empty() {
            summary.push_str(&format!(" Топ приложений: {}.", top.join(", ")));
        }
    }

    Ok(summary)
}

fn spawn_summary(app: tauri::AppHandle, days: i64, label: &'static str, kind: &'static str) {
    tokio::spawn(async move {
        let r = async {
            let summary = period_summary(app.state::<SqlitePool>().inner(), days, label).await?;
            ask_ai(&app, SYSTEM_SUMMARY, &summary).await
        }
        .await;
        let (result, error) = match r {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        let _ = app.emit("period-summary", SummaryPayload { kind: kind.into(), result, error });
    });
}

#[tauri::command]
pub async fn summarize_day(app: tauri::AppHandle) -> Result<(), String> {
    spawn_summary(app, 1, "последние сутки", "day");
    Ok(())
}

#[tauri::command]
pub async fn summarize_week(app: tauri::AppHandle) -> Result<(), String> {
    spawn_summary(app, 7, "последняя неделя", "week");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_order_from_cloud_primary() {
        // Основной openai: облако первым, потом второй ключ, потом локалка
        assert_eq!(
            resolve_provider_order("openai", true, true, true),
            vec![Provider::OpenAi, Provider::Anthropic, Provider::Local]
        );
        assert_eq!(
            resolve_provider_order("anthropic", true, true, true),
            vec![Provider::Anthropic, Provider::OpenAi, Provider::Local]
        );
    }

    #[test]
    fn fallback_order_from_local_primary() {
        assert_eq!(
            resolve_provider_order("local", true, true, false),
            vec![Provider::Local, Provider::OpenAi]
        );
    }

    #[test]
    fn unavailable_providers_are_skipped() {
        // Нет локальной модели и нет ключа anthropic
        assert_eq!(
            resolve_provider_order("openai", false, true, false),
            vec![Provider::OpenAi]
        );
        // Основной без ключа: сразу откат на доступного
        assert_eq!(
            resolve_provider_order("openai", true, false, false),
            vec![Provider::Local]
        );
    }

    #[test]
    fn nothing_available_is_empty() {
        assert_eq!(resolve_provider_order("local", false, false, false), vec![]);
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_completed_task(pool: &SqlitePool, title: &str, completed_at: &str) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden, created_at, updated_at, completed_at)
             VALUES (?, ?, 'Done', 'Medium', 'Work', 'None', '[]', 0, ?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(title)
            .bind(completed_at).bind(completed_at).bind(completed_at)
            .execute(pool).await.unwrap();
    }

    async fn insert_activity(pool: &SqlitePool, timestamp: &str, state: &str, secs: i64) {
        sqlx::query(
            "INSERT INTO activity_log (timestamp, state, app_focused, input_events, duration_secs)
             VALUES (?, ?, 1, 1, ?)")
            .bind(timestamp).bind(state).bind(secs)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn insight_summary_empty_db_reports_no_data() {
        let pool = test_pool().await;
        let s = insight_summary(&pool).await.unwrap();
        assert!(s.contains("Активные минуты по дням: нет данных"), "{s}");
        assert!(s.contains("Выполнено задач за последние дни: 0"), "{s}");
        assert!(s.contains("Топ-категория выполненных задач: нет данных"), "{s}");
    }

    #[tokio::test]
    async fn insight_summary_includes_activity_and_completions() {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        insert_activity(&pool, &now, "Active", 600).await;
        insert_completed_task(&pool, "готово", &now).await;

        let s = insight_summary(&pool).await.unwrap();
        assert!(s.contains("10 мин"), "{s}");
        assert!(s.contains("Выполнено задач за последние дни: 1"), "{s}");
        assert!(s.contains("Топ-категория выполненных задач: Work"), "{s}");
    }

    #[tokio::test]
    async fn period_summary_empty_db() {
        let pool = test_pool().await;
        let s = period_summary(&pool, 1, "день").await.unwrap();
        assert!(s.contains("Период: день"), "{s}");
        assert!(s.contains("Выполнено задач: 0."), "{s}");
        assert!(s.contains("Активное время: 0 мин"), "{s}");
        assert!(s.contains("Просрочено сейчас: 0"), "{s}");
    }

    #[tokio::test]
    async fn period_summary_counts_only_period_and_active_state() {
        let pool = test_pool().await;
        let now = chrono::Utc::now();
        let recent = now.to_rfc3339();
        let old = (now - chrono::Duration::days(30)).to_rfc3339();

        insert_completed_task(&pool, "свежая", &recent).await;
        insert_completed_task(&pool, "старая", &old).await; // вне периода
        insert_activity(&pool, &recent, "Active", 300).await;
        insert_activity(&pool, &recent, "Idle", 3600).await; // Idle не считается
        insert_activity(&pool, &old, "Active", 3600).await; // вне периода

        let s = period_summary(&pool, 7, "неделя").await.unwrap();
        assert!(s.contains("Выполнено задач: 1 (свежая)"), "{s}");
        assert!(!s.contains("старая"), "{s}");
        assert!(s.contains("Активное время: 5 мин"), "{s}");
    }
}
