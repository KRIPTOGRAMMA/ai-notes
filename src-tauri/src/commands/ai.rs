use tauri::Emitter;
use sqlx::SqlitePool;
use tauri::Manager;
use serde::Serialize;
use crate::ai::sidecar::{SharedSidecar, ensure_running};
use crate::ai::engine::ask;
use crate::ai::cloud::{ask_openai, ask_anthropic};

// Промпты, чей ответ читает пользователь, существуют парами: русской и
// английской (v0.9.42). Приписать требование языка к русскому промпту, как
// делал v0.9.41, оказалось недостаточно — проверено на живой модели: ответ
// приходил русским и при английском интерфейсе. Основной текст промпта
// перевешивает одну строку-требование в конце, поэтому язык задаёт сам
// промпт целиком.
const SYSTEM_REWRITE: Prompt = Prompt {
    ru: "Перепиши задачу в SMART-формат: чёткая цель, измеримый результат, срок. Только результат, без пояснений.",
    en: "Rewrite the task in SMART format: a clear goal, a measurable result, a deadline. Reply with the result only, no explanations.",
};

const SYSTEM_SUBTASKS: &str =
    "You are a task planner. Split the task into 3-7 subtasks. Reply ONLY with a JSON array of strings, nothing else. Example: [\"subtask 1\", \"subtask 2\", \"subtask 3\"]";

const SYSTEM_INSIGHT: Prompt = Prompt {
    ru: "Ты ассистент по продуктивности. Дай 1–3 коротких предложения про продуктивность пользователя. Только текст, без пояснений и списков.",
    en: "You are a productivity assistant. Give 1-3 short sentences about the user's productivity. Text only, no explanations and no lists.",
};

const SYSTEM_SUMMARY: Prompt = Prompt {
    ru: "Ты ассистент по продуктивности. Составь краткое резюме периода (3–5 предложений): что сделано, сколько активного времени, прогресс целей (если есть), что требует внимания. Только текст.",
    en: "You are a productivity assistant. Write a short summary of the period (3-5 sentences): what was done, how much active time, progress on goals (if any), what needs attention. Text only.",
};

// ИИ по выделению в редакторе заметок (v0.9.09): выделил текст -> одно из
// действий ниже -> модель возвращает только заменяющий текст, без пояснений
// и без markdown-обёртки цитаты — иначе пришлось бы чистить ответ так же,
// как parse_subtasks чистит списки.
const SYSTEM_SELECTION_REWRITE: &str =
    "Перепиши следующий фрагмент текста, сохранив смысл и язык оригинала, но улучшив стиль и ясность. Ответь только новым текстом, без пояснений и кавычек.";
const SYSTEM_SELECTION_SHORTEN: &str =
    "Сократи следующий фрагмент текста, сохранив ключевой смысл и язык оригинала. Ответь только новым текстом, без пояснений и кавычек.";
const SYSTEM_SELECTION_EXPAND: &str =
    "Разверни следующий фрагмент текста, добавив уместные детали, сохранив язык оригинала. Ответь только новым текстом, без пояснений и кавычек.";
const SYSTEM_SELECTION_GRAMMAR: &str =
    "Исправь грамматику, орфографию и пунктуацию в следующем фрагменте текста, не меняя смысл и стиль. Ответь только исправленным текстом, без пояснений и кавычек.";

#[derive(Clone, Serialize)]
pub struct SelectionEditPayload {
    pub request_id: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

fn selection_system_prompt(mode: &str) -> Result<&'static str, String> {
    match mode {
        "rewrite" => Ok(SYSTEM_SELECTION_REWRITE),
        "shorten" => Ok(SYSTEM_SELECTION_SHORTEN),
        "expand" => Ok(SYSTEM_SELECTION_EXPAND),
        "grammar" => Ok(SYSTEM_SELECTION_GRAMMAR),
        _ => Err(format!("Неизвестное действие: {mode}")),
    }
}

#[tauri::command]
pub async fn ai_edit_selection(
    app: tauri::AppHandle,
    request_id: String,
    text: String,
    mode: String,
) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let system = selection_system_prompt(&mode)?;
            // verbatim: промпты выделения сами требуют «сохранив язык
            // оригинала». Навязать сюда язык интерфейса значило бы
            // переводить чужой текст по нажатию «сократить».
            ask_ai_verbatim(&app, system, &text).await
        }.await;
        let payload = match r {
            Ok(result) => SelectionEditPayload { request_id, result: Some(strip_wrapping(&result).to_string()), error: None },
            Err(e) => SelectionEditPayload { request_id, result: None, error: Some(e) },
        };
        let _ = app.emit("ai-selection-result", payload);
    });
    Ok(())
}

// ИИ: резюме длинной заметки (v0.9.10) — кнопка сжимает содержимое заметки
// в 3-5 пунктов. Отдельный промпт от SYSTEM_SUMMARY (тот — про сводку
// активности/задач за период, а не про текст произвольной заметки).
const SYSTEM_NOTE_SUMMARY: &str =
    "Сожми следующую заметку в 3-5 кратких пунктов с ключевыми мыслями. Ответь только списком пунктов \
через перенос строки, каждый начинается с \"- \", без вступления и пояснений, на языке заметки.";

#[derive(Clone, Serialize)]
pub struct NoteSummaryPayload {
    pub request_id: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ai_summarize_note(app: tauri::AppHandle, request_id: String, text: String) -> Result<(), String> {
    tokio::spawn(async move {
        // verbatim: промпт требует «на языке заметки» — резюме русской
        // заметки не должно приходить по-английски из-за языка интерфейса.
        let r = ask_ai_verbatim(&app, SYSTEM_NOTE_SUMMARY, &text).await;
        let payload = match r {
            Ok(result) => NoteSummaryPayload { request_id, result: Some(result.trim().to_string()), error: None },
            Err(e) => NoteSummaryPayload { request_id, result: None, error: Some(e) },
        };
        let _ = app.emit("ai-note-summary", payload);
    });
    Ok(())
}

// ИИ: извлечение задач из заметки (v0.9.11) — кнопка в редакторе предлагает
// список задач по тексту заметки (особенно полезно для daily note),
// подтверждение создаёт их. Тот же suggest-then-confirm, что подзадачи
// задачи (ai_subtasks) — и тот же промпт/парсер: "предложи 3-7 пунктов
// действий" по смыслу совпадает с "раздели на подзадачи", а parse_subtasks
// уже умеет надёжно чистить JSON/нумерацию/маркеры/мусор модели.
const SYSTEM_EXTRACT_TASKS: &str =
    "You are a task planner. Read the note below and extract concrete actionable tasks mentioned or implied \
in it (things to do), 1-7 items. Reply ONLY with a JSON array of strings, nothing else. If there are no \
actionable tasks, reply with an empty array []. Example: [\"task 1\", \"task 2\"]";

#[derive(Clone, Serialize)]
pub struct ExtractTasksPayload {
    pub request_id: String,
    pub items: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ai_extract_tasks(app: tauri::AppHandle, request_id: String, text: String) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let raw = ask_ai(&app, SYSTEM_EXTRACT_TASKS, &text).await?;
            match parse_subtasks(&raw) {
                Some(joined) => Ok(joined.split("|||").map(|s| s.to_string()).collect::<Vec<_>>()),
                None => Ok(vec![]), // пустой список — не ошибка, просто нечего извлекать
            }
        }.await;
        let payload = match r {
            Ok(items) => ExtractTasksPayload { request_id, items, error: None },
            Err(e) => ExtractTasksPayload { request_id, items: vec![], error: Some(e) },
        };
        let _ = app.emit("ai-extract-tasks", payload);
    });
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct AiResult {
    pub task_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

// Убирает по одной паре внешних markdown/кавычка-символов с концов строки:
// "**текст**" -> "текст", "`текст`" -> "текст", «текст» -> "текст".
fn strip_wrapping(s: &str) -> &str {
    let s = s.trim();
    for (open, close) in [("**", "**"), ("__", "__"), ("`", "`"), ("«", "»"), ("\"", "\""), ("'", "'")] {
        if s.len() > open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            return s[open.len()..s.len() - close.len()].trim();
        }
    }
    s
}

// Строка-мусор, которую модель могла добавить вокруг настоящих подзадач:
// кодовый забор (``` или ```json) и типичные преамбулы/пустые заголовки.
fn is_noise_line(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() { return true; }
    if l.starts_with("```") { return true; }
    let lower = l.to_lowercase();
    let colon_like_ending = l.ends_with(':') || l.ends_with('：');
    if colon_like_ending && lower.len() < 80 { return true; } // «Вот список подзадач:» и т.п.
    false
}

// Один пункт списка -> очищенный текст подзадачи, либо None если после чистки
// ничего разумного не осталось (пустая строка, кодовый забор, чистая пунктуация,
// либо строка — на самом деле кусок JSON, а не текст подзадачи).
fn clean_subtask_line(line: &str) -> Option<String> {
    let l = line.trim();
    if is_noise_line(l) { return None; }
    // Нумерация ("1." "2)") и маркер списка ("-", "•", одиночная "* "), но не
    // пара "**" — это markdown-жирное, которое должно остаться нетронутым
    // для strip_wrapping ниже.
    let stripped = l
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches(['.', ')'])
        .trim_start();
    let stripped = if let Some(rest) = stripped.strip_prefix("* ") {
        rest
    } else {
        stripped.trim_start_matches(['-', '•'])
    };
    let stripped = stripped.trim();
    let stripped = strip_wrapping(stripped);
    if stripped.is_empty() || is_noise_line(stripped) { return None; }
    // Похоже на сырой JSON-литерал ("[1, 2, 3]", "{...}"), а не на текст
    // подзадачи — модель могла не собрать валидный массив строк.
    let looks_like_json = (stripped.starts_with('[') && stripped.ends_with(']'))
        || (stripped.starts_with('{') && stripped.ends_with('}'));
    if looks_like_json { return None; }
    Some(stripped.to_string())
}

const MAX_SUBTASKS: usize = 15; // защита от зацикленного/мусорного ответа модели

// Строгий разбор ответа модели в список подзадач: не доверяем модели —
// как parse_plan в planner.rs, при малейшем сомнении отбрасываем элемент,
// а не пытаемся угадать. Оба пути (JSON и построчный фолбэк) прогоняются
// через одну и ту же чистку/фильтрацию мусора.
fn parse_subtasks(raw: &str) -> Option<String> {
    let trimmed = raw.trim();

    // JSON-массив: ищем сбалансированную пару [...] (не просто первую [ и
    // последнюю ] — иначе строка вида "сделай [важно]: [\"a\",\"b\"]" ломается),
    // пробуя от каждого '[' до последнего ']' после него, пока разбор не удастся.
    let brackets: Vec<usize> = trimmed.match_indices('[').map(|(i, _)| i).collect();
    let last_close = trimmed.rfind(']');
    if let Some(end) = last_close {
        for start in brackets {
            if start >= end { continue; }
            let Ok(items) = serde_json::from_str::<Vec<String>>(&trimmed[start..=end]) else { continue };
            let cleaned: Vec<String> = items.iter().filter_map(|s| clean_subtask_line(s)).collect();
            if !cleaned.is_empty() {
                let mut cleaned = cleaned;
                cleaned.truncate(MAX_SUBTASKS);
                return Some(cleaned.join("|||"));
            }
        }
    }

    // Фолбэк: построчный список (нумерация/маркеры/markdown), с той же чисткой.
    let mut items: Vec<String> = trimmed.lines().filter_map(clean_subtask_line).collect();
    if items.is_empty() { return None; }
    items.truncate(MAX_SUBTASKS);
    Some(items.join("|||"))
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

// Язык ответа модели (v0.9.41 → переделано в v0.9.42).
//
// v0.9.41 дописывала к русскому промпту строку «Reply in English.» и на этом
// останавливалась. На живой модели это не сработало: ответ приходил русским
// и при английском интерфейсе. Причина в том, что модель ориентируется на
// язык основного текста инструкции, а не на одну строку в конце — особенно
// малые локальные GGUF. Поэтому язык теперь задаёт сам промпт целиком:
// промпты для человека объявлены парами _RU/_EN.
//
// Приписка сохранена как второй, дублирующий сигнал — она ничего не стоит и
// помогает в пограничных случаях (например, русские названия задач внутри
// английского промпта).
//
// ВАЖНО: промпты, работающие с текстом заметки (резюме, переписать, сжать,
// исправить грамматику), сюда не попадают — они обязаны отвечать на языке
// самого текста. Менять язык чужой заметки при нажатии «сократить» — баг, а
// не локализация. Такие вызовы идут через ask_ai_verbatim.
fn with_lang(system: &str, lang: crate::i18n::Lang) -> String {
    let req = match lang {
        crate::i18n::Lang::Ru => "Отвечай на русском языке.",
        crate::i18n::Lang::En => "Reply in English.",
    };
    format!("{system}\n{req}")
}

/// Пара промптов «русский / английский» для одного действия.
pub struct Prompt {
    pub ru: &'static str,
    pub en: &'static str,
}

impl Prompt {
    fn pick(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::Ru => self.ru,
            crate::i18n::Lang::En => self.en,
        }
    }
}

/// Спросить модель, не навязывая язык ответа. Для промптов, где язык задаёт
/// содержимое запроса (текст заметки), а не интерфейс.
pub async fn ask_ai_verbatim(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    ask_ai_inner(app, system.to_string(), user).await
}

/// Спросить модель на языке интерфейса: выбирается промпт нужного языка,
/// плюс дублирующее требование в конце.
pub async fn ask_ai_localized(app: &tauri::AppHandle, prompt: &Prompt, user: &str) -> Result<String, String> {
    let lang = crate::i18n::current_lang(app.state::<SqlitePool>().inner()).await;
    ask_ai_inner(app, with_lang(prompt.pick(lang), lang), user).await
}

/// Промпт без языковой пары (машинный JSON, где текст один на все языки),
/// но с требованием языка — содержимое ответа читает человек.
pub async fn ask_ai(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    let lang = crate::i18n::current_lang(app.state::<SqlitePool>().inner()).await;
    ask_ai_inner(app, with_lang(system, lang), user).await
}

async fn ask_ai_inner(app: &tauri::AppHandle, system: String, user: &str) -> Result<String, String> {
    let system: &str = &system;
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
        let r = ask_ai_localized(&app, &SYSTEM_REWRITE, &title).await;
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
pub async fn ai_classify(
    app: tauri::AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
    task_id: String,
    title: String,
) -> Result<(), String> {
    let pool = pool.inner().clone();
    tokio::spawn(async move {
        // Категории пользовательские (v0.6.3): промпт строится из таблицы,
        // ответ модели сопоставляется с ней и наружу уходит id категории.
        let r = async {
            let cats = crate::commands::categories::get_categories_impl(&pool)
                .await
                .map_err(|e| e.to_string())?;
            let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
            let system = format!(
                "Категория задачи — одна из: {}. Ответь одним словом из списка, без пояснений.",
                names.join(", ")
            );
            let answer = ask_ai(&app, &system, &title).await?;
            crate::commands::categories::match_category(&cats, &answer)
                .ok_or_else(|| format!("Модель ответила «{}» — категория не распознана", answer.trim()))
        }.await;
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
//
// Язык контекста обязан совпадать с языком промпта (v0.9.43). Английская
// инструкция поверх русского контекста не работает: модель видит стену
// русских данных («Активные минуты по дням…», названия задач, категории) и
// отвечает на её языке — проверено пользователем на локальной модели, где
// инсайт и сводка оставались русскими, хотя короткий SMART-промпт без
// контекста переводился нормально.
async fn insight_summary(pool: &SqlitePool, lang: crate::i18n::Lang) -> Result<String, String> {
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
        .map(|d| crate::i18n::tr_args("{date}: {n} мин", lang, &[("date", d.date.clone()), ("n", d.minutes.to_string())]))
        .collect();
    let done_recent: i64 = completions.iter().rev().take(7).map(|c| c.completed).sum();
    let no_data = crate::i18n::tr("нет данных", lang);
    let top_cat = cats
        .iter()
        .max_by_key(|c| c.count)
        .map(|c| c.category.clone())
        .unwrap_or_else(|| no_data.clone());

    Ok(crate::i18n::tr_args(
        "Активные минуты по дням: {mins}. Выполнено задач за последние дни: {done}. Топ-категория выполненных задач: {cat}.",
        lang,
        &[
            ("mins", if minutes.is_empty() { no_data.clone() } else { minutes.join(", ") }),
            ("done", done_recent.to_string()),
            ("cat", top_cat),
        ],
    ))
}

#[tauri::command]
pub async fn dashboard_insight(app: tauri::AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let pool = app.state::<SqlitePool>();
            let lang = crate::i18n::current_lang(pool.inner()).await;
            let summary = insight_summary(pool.inner(), lang).await?;
            ask_ai_localized(&app, &SYSTEM_INSIGHT, &summary).await
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
async fn period_summary(pool: &SqlitePool, days: i64, label: &str, lang: crate::i18n::Lang) -> Result<String, String> {
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

    let mut summary = crate::i18n::tr_args(
        "Период: {label}. Выполнено задач: {done}{titles}. Активное время: {mins} мин. Просрочено сейчас: {overdue}.",
        lang,
        &[
            ("label", crate::i18n::tr(label, lang)),
            ("done", done.len().to_string()),
            ("titles", if done.is_empty() { String::new() } else { format!(" ({})", done.join(", ")) }),
            ("mins", active_mins.to_string()),
            ("overdue", overdue.to_string()),
        ],
    );

    // Недельное ревью (v0.5.6): цели проектов и топ приложений — данные фаз 1–2
    if days >= 7 {
        let projects = crate::commands::projects::get_projects_impl(pool).await.unwrap_or_default();
        let goals: Vec<String> = projects
            .iter()
            .filter(|p| !p.archived && (p.goal_tasks.is_some() || p.goal_mins.is_some()))
            .map(|p| {
                let mut parts = vec![];
                if let Some(n) = p.goal_tasks {
                    parts.push(crate::i18n::tr_args("{done}/{total} задач", lang, &[("done", p.goal_done_tasks.to_string()), ("total", n.to_string())]));
                }
                if let Some(n) = p.goal_mins {
                    parts.push(crate::i18n::tr_args("{done}/{total} мин", lang, &[("done", p.goal_done_mins.to_string()), ("total", n.to_string())]));
                }
                format!("{}: {}", p.name, parts.join(", "))
            })
            .collect();
        if !goals.is_empty() {
            summary.push_str(&crate::i18n::tr_args(" Цели проектов: {goals}.", lang, &[("goals", goals.join("; "))]));
        }

        let apps = crate::commands::monitor::get_app_usage_impl(pool, 7).await.unwrap_or_default();
        let top: Vec<String> = apps.iter().take(3)
            .map(|a| crate::i18n::tr_args("{app} ({n} мин)", lang, &[("app", a.app.clone()), ("n", a.minutes.to_string())]))
            .collect();
        if !top.is_empty() {
            summary.push_str(&crate::i18n::tr_args(" Топ приложений: {apps}.", lang, &[("apps", top.join(", "))]));
        }
    }

    Ok(summary)
}

fn spawn_summary(app: tauri::AppHandle, days: i64, label: &'static str, kind: &'static str) {
    tokio::spawn(async move {
        let r = async {
            let pool = app.state::<SqlitePool>();
            let lang = crate::i18n::current_lang(pool.inner()).await;
            let summary = period_summary(pool.inner(), days, label, lang).await?;
            ask_ai_localized(&app, &SYSTEM_SUMMARY, &summary).await
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
    fn lang_requirement_appended_per_language() {
        let ru = with_lang("Ты ассистент.", crate::i18n::Lang::Ru);
        let en = with_lang("Ты ассистент.", crate::i18n::Lang::En);
        assert!(ru.contains("Отвечай на русском языке."), "{ru}");
        assert!(en.contains("Reply in English."), "{en}");
        // исходный промпт не потерян
        assert!(ru.starts_with("Ты ассистент."));
        assert!(en.starts_with("Ты ассистент."));
        // и языки не перепутаны
        assert!(!en.contains("Отвечай на русском"));
        assert!(!ru.contains("Reply in English"));
    }

    // verbatim-промпты обязаны молчать про язык: их язык задаёт содержимое
    // запроса (текст заметки) или он вообще не важен (машинный JSON).
    // Требование «по-русски» здесь сломало бы правку английской заметки.
    #[test]
    fn verbatim_prompts_do_not_demand_a_language() {
        let sources = [
            include_str!("ai.rs"),
            include_str!("planner.rs"),
            include_str!("note_links.rs"),
        ];
        for src in sources {
            for (idx, _) in src.match_indices("const SYSTEM_") {
                let rest = &src[idx..];
                // пары _RU/_EN объявлены как `Prompt {` — они язык задают
                // намеренно, проверка не про них
                let head = rest.lines().next().unwrap_or("");
                if head.contains("Prompt") {
                    continue;
                }
                let end = rest.find("\";").map(|i| i + 2).unwrap_or(rest.len());
                let decl = &rest[..end];
                for banned in ["по-русски", "По-русски", "на русском", "in Russian", "in English"] {
                    assert!(
                        !decl.contains(banned),
                        "verbatim-промпт задаёт язык: «{banned}» в {head}"
                    );
                }
            }
        }
    }

    // Главный инвариант v0.9.42: у каждой пары обе стороны заполнены и
    // действительно различаются. Скопировать русский текст в поле en (или
    // забыть его заполнить) — самый вероятный способ незаметно вернуть
    // прежний баг: интерфейс английский, ответ русский.
    #[test]
    fn localized_prompt_pairs_are_filled_and_distinct() {
        let pairs: [(&str, &Prompt); 4] = [
            ("SYSTEM_REWRITE", &SYSTEM_REWRITE),
            ("SYSTEM_INSIGHT", &SYSTEM_INSIGHT),
            ("SYSTEM_SUMMARY", &SYSTEM_SUMMARY),
            ("SYSTEM_WHAT_NOW", &crate::commands::planner::SYSTEM_WHAT_NOW),
        ];
        for (name, p) in pairs {
            assert!(!p.ru.trim().is_empty(), "{name}: пустой русский промпт");
            assert!(!p.en.trim().is_empty(), "{name}: пустой английский промпт");
            assert_ne!(p.ru, p.en, "{name}: en скопирован с ru");
            // русская сторона обязана быть русской, английская — без кириллицы
            assert!(
                p.ru.chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)),
                "{name}: в поле ru нет кириллицы"
            );
            assert!(
                !p.en.chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)),
                "{name}: в поле en осталась кириллица"
            );
        }
    }

    // Пары мало объявить — их надо и вызывать через ask_ai_localized.
    // Обращение к `.ru`/`.en` на месте вызова прибивает язык намертво, минуя
    // настройку, и это ровно тот баг, который чинит версия: объявленная
    // пара при этом выглядит корректной, а пользователь всё равно получает
    // один язык. Проверка по исходникам, потому что вызовы асинхронные и
    // требуют живого AppHandle.
    #[test]
    fn localized_prompts_are_not_pinned_to_one_side_at_call_sites() {
        let sources = [
            ("ai.rs", include_str!("ai.rs")),
            ("planner.rs", include_str!("planner.rs")),
        ];
        for (name, src) in sources {
            for (idx, _) in src.match_indices("SYSTEM_") {
                let rest = &src[idx..];
                let end = rest.find(|c: char| c == ')' || c == ',' || c == '\n').unwrap_or(rest.len());
                let frag = &rest[..end];
                // объявление пары (`ru:`/`en:` внутри Prompt {}) — не вызов
                if frag.contains("Prompt") {
                    continue;
                }
                for pinned in [".ru", ".en"] {
                    // в тестах обращение к полям законно — там сверяют пары
                    let in_test_mod = idx > src.find("#[cfg(test)]").unwrap_or(usize::MAX);
                    assert!(
                        !frag.contains(pinned) || in_test_mod,
                        "{name}: язык прибит на месте вызова — «{frag}»"
                    );
                }
            }
        }
    }

    #[test]
    fn prompt_pick_follows_language() {
        assert_eq!(SYSTEM_INSIGHT.pick(crate::i18n::Lang::Ru), SYSTEM_INSIGHT.ru);
        assert_eq!(SYSTEM_INSIGHT.pick(crate::i18n::Lang::En), SYSTEM_INSIGHT.en);
    }

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
        let s = insight_summary(&pool, crate::i18n::Lang::Ru).await.unwrap();
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

        let s = insight_summary(&pool, crate::i18n::Lang::Ru).await.unwrap();
        assert!(s.contains("10 мин"), "{s}");
        assert!(s.contains("Выполнено задач за последние дни: 1"), "{s}");
        assert!(s.contains("Топ-категория выполненных задач: Work"), "{s}");
    }

    // Главный инвариант v0.9.43: контекст, уходящий в промпт, обязан быть на
    // языке интерфейса. Английская инструкция поверх русского контекста не
    // работает — проверено пользователем на локальной модели: инсайт и сводка
    // оставались русскими, хотя короткий SMART-промпт (там контекста нет)
    // переводился нормально. Модель следует языку данных, а не инструкции.
    #[tokio::test]
    async fn english_context_has_no_russian_left() {
        let pool = test_pool().await;
        let now = chrono::Utc::now().to_rfc3339();
        insert_activity(&pool, &now, "Active", 600).await;
        insert_completed_task(&pool, "task done", &now).await;

        let has_cyrillic =
            |s: &str| s.chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c));

        let insight = insight_summary(&pool, crate::i18n::Lang::En).await.unwrap();
        assert!(!has_cyrillic(&insight), "кириллица в английском инсайте: {insight}");

        // days >= 7 включает ветки целей проектов и топа приложений
        let period = period_summary(&pool, 7, "последняя неделя", crate::i18n::Lang::En)
            .await
            .unwrap();
        assert!(!has_cyrillic(&period), "кириллица в английской сводке: {period}");
        // метка периода тоже переведена, а не оставлена ключом
        assert!(period.contains("the last week"), "{period}");
    }

    #[tokio::test]
    async fn period_summary_empty_db() {
        let pool = test_pool().await;
        let s = period_summary(&pool, 1, "день", crate::i18n::Lang::Ru).await.unwrap();
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

        let s = period_summary(&pool, 7, "неделя", crate::i18n::Lang::Ru).await.unwrap();
        assert!(s.contains("Выполнено задач: 1 (свежая)"), "{s}");
        assert!(!s.contains("старая"), "{s}");
        assert!(s.contains("Активное время: 5 мин"), "{s}");
    }

    // --- parse_subtasks: v0.8.11, "модели не доверять" — грязные ответы LLM ---

    #[test]
    fn parses_clean_json_array() {
        let raw = r#"["Собрать требования", "Написать код", "Написать тесты"]"#;
        assert_eq!(parse_subtasks(raw).unwrap(), "Собрать требования|||Написать код|||Написать тесты");
    }

    #[test]
    fn strips_code_fence_around_json() {
        let raw = "```json\n[\"шаг раз\", \"шаг два\"]\n```";
        assert_eq!(parse_subtasks(raw).unwrap(), "шаг раз|||шаг два");
    }

    #[test]
    fn preamble_line_is_not_included_as_subtask() {
        let raw = "Вот список подзадач:\n1. Изучить требования\n2. Написать план\n3. Реализовать и протестировать";
        let result = parse_subtasks(raw).unwrap();
        assert!(!result.contains("Вот список подзадач"), "{result}");
        assert_eq!(result, "Изучить требования|||Написать план|||Реализовать и протестировать");
    }

    #[test]
    fn stray_bracket_before_real_json_array_does_not_break_parsing() {
        let raw = r#"Разбил задачу [важно] на подзадачи: ["шаг 1", "шаг 2"]"#;
        assert_eq!(parse_subtasks(raw).unwrap(), "шаг 1|||шаг 2");
    }

    #[test]
    fn empty_json_array_falls_back_to_none_not_garbage() {
        assert_eq!(parse_subtasks("[]"), None);
    }

    #[test]
    fn json_array_of_non_strings_does_not_produce_garbage_literal() {
        // Раньше "[1, 2, 3]" целиком выживало как один поддельный пункт.
        assert_eq!(parse_subtasks("[1, 2, 3]"), None);
    }

    #[test]
    fn numbered_list_with_markdown_bold_is_cleaned() {
        let raw = "1. **Собрать требования**\n2. **Написать код**";
        assert_eq!(parse_subtasks(raw).unwrap(), "Собрать требования|||Написать код");
    }

    #[test]
    fn bullet_markers_are_stripped() {
        let raw = "- шаг раз\n* шаг два\n• шаг три";
        assert_eq!(parse_subtasks(raw).unwrap(), "шаг раз|||шаг два|||шаг три");
    }

    #[test]
    fn pure_garbage_or_whitespace_returns_none() {
        assert_eq!(parse_subtasks(""), None);
        assert_eq!(parse_subtasks("   \n  \n "), None);
        assert_eq!(parse_subtasks("```\n```"), None);
    }

    #[test]
    fn excessive_item_count_is_truncated() {
        let items: Vec<String> = (1..=30).map(|i| format!("шаг {i}")).collect();
        let raw = serde_json::to_string(&items).unwrap();
        let result = parse_subtasks(&raw).unwrap();
        assert_eq!(result.split("|||").count(), MAX_SUBTASKS);
    }

    // --- ИИ по выделению (v0.9.09) ---

    #[test]
    fn selection_mode_maps_to_distinct_prompts() {
        let rewrite = selection_system_prompt("rewrite").unwrap();
        let shorten = selection_system_prompt("shorten").unwrap();
        let expand = selection_system_prompt("expand").unwrap();
        let grammar = selection_system_prompt("grammar").unwrap();
        let all = [rewrite, shorten, expand, grammar];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j { assert_ne!(a, b); }
            }
        }
    }

    #[test]
    fn unknown_selection_mode_is_rejected() {
        assert!(selection_system_prompt("delete-everything").is_err());
    }
}
