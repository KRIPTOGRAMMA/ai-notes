use tauri::Emitter;
use sqlx::SqlitePool;
use tauri::Manager;
use serde::Serialize;
use crate::ai::sidecar::{SharedSidecar, ensure_running};
use crate::ai::engine::ask;
use crate::ai::cloud::{ask_openai, ask_anthropic};

// Prompts whose answer the user reads exist in pairs, Russian and English.
// Appending a language requirement to the Russian prompt was not enough —
// verified against a live model: the answer still came back in Russian even
// with an English interface. The body of the prompt outweighs a single
// requirement line at the end, so the language is set by the whole prompt.
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

// AI on an editor selection: select text -> pick one of the actions below ->
// the model returns only the replacement text, with no commentary and no
// markdown quote wrapper. Otherwise the answer would need the same scrubbing
// that parse_subtasks applies to lists.
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
            // verbatim: the selection prompts already require "keeping the
            // original language". Forcing the interface language here would
            // translate someone's text at the press of "shorten".
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

// AI: summary of a long note — the button compresses the note into 3-5 bullet
// points. A separate prompt from SYSTEM_SUMMARY, which covers an activity/task
// digest for a period rather than the text of an arbitrary note.
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
        // verbatim: the prompt requires "in the language of the note" — a
        // summary of a Russian note must not arrive in English merely because
        // the interface is English.
        let r = ask_ai_verbatim(&app, SYSTEM_NOTE_SUMMARY, &text).await;
        let payload = match r {
            Ok(result) => NoteSummaryPayload { request_id, result: Some(result.trim().to_string()), error: None },
            Err(e) => NoteSummaryPayload { request_id, result: None, error: Some(e) },
        };
        let _ = app.emit("ai-note-summary", payload);
    });
    Ok(())
}

// AI: extracting tasks from a note — the editor button proposes a list of
// tasks based on the note text (especially useful for a daily note), and
// confirming creates them. The same suggest-then-confirm flow as task subtasks
// (ai_subtasks), and the same prompt and parser: "propose 3-7 action items"
// means the same thing as "split into subtasks", and parse_subtasks already
// scrubs JSON, numbering, bullets and model noise reliably.
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
                None => Ok(vec![]), // an empty list is not an error, there is simply nothing to extract
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

// Strips one pair of outer markdown/quote characters from the ends of a line:
// "**text**" -> "text", "`text`" -> "text", "«text»" -> "text".
fn strip_wrapping(s: &str) -> &str {
    let s = s.trim();
    for (open, close) in [("**", "**"), ("__", "__"), ("`", "`"), ("«", "»"), ("\"", "\""), ("'", "'")] {
        if s.len() > open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            return s[open.len()..s.len() - close.len()].trim();
        }
    }
    s
}

// A junk line the model may have added around the real subtasks: a code fence
// (``` or ```json) and the usual preambles or empty headings.
fn is_noise_line(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() { return true; }
    if l.starts_with("```") { return true; }
    let lower = l.to_lowercase();
    let colon_like_ending = l.ends_with(':') || l.ends_with('：');
    if colon_like_ending && lower.len() < 80 { return true; } // "Here is the list of subtasks:" and the like
    false
}

// One list item -> the cleaned subtask text, or None when nothing sensible
// survives the scrubbing (an empty line, a code fence, bare punctuation, or a
// line that is really a piece of JSON rather than subtask text).
fn clean_subtask_line(line: &str) -> Option<String> {
    let l = line.trim();
    if is_noise_line(l) { return None; }
    // Numbering ("1." "2)") and list bullets ("-", "•", a lone "* "), but not a
    // "**" pair — that is markdown bold and must reach strip_wrapping below
    // untouched.
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
    // Looks like a raw JSON literal ("[1, 2, 3]", "{...}") rather than subtask
    // text — the model may have failed to build a valid array of strings.
    let looks_like_json = (stripped.starts_with('[') && stripped.ends_with(']'))
        || (stripped.starts_with('{') && stripped.ends_with('}'));
    if looks_like_json { return None; }
    Some(stripped.to_string())
}

const MAX_SUBTASKS: usize = 15; // guards against a looping or garbage answer from the model

// Strict parsing of the model's answer into a list of subtasks. We do not
// trust the model: like parse_plan in planner.rs, at the slightest doubt an
// item is dropped rather than guessed at. Both paths (JSON and the line-by-line
// fallback) run through the same scrubbing and junk filtering.
fn parse_subtasks(raw: &str) -> Option<String> {
    let trimmed = raw.trim();

    // JSON array: look for a balanced [...] pair rather than simply the first [
    // and the last ], which would break on a string like
    // "do [important]: [\"a\",\"b\"]". We try from each '[' to the last ']' after
    // it until parsing succeeds.
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

    // Fallback: a line-by-line list (numbering/bullets/markdown), same scrubbing.
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

// The order providers are tried in when auto-switching: starting from the
// chosen primary, with unavailable ones (no key / no model.gguf) dropped
// immediately. A pure function.
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

// The language of the model's answer.
//
// The first attempt appended a "Reply in English." line to the Russian prompt
// and stopped there. Against a live model that did not work: the answer came
// back in Russian even with an English interface. The model follows the
// language of the instruction's main text rather than one line at the end —
// small local GGUF models especially. So the language is now set by the whole
// prompt: prompts meant for a human are declared as _RU/_EN pairs.
//
// The appended line is kept as a second, duplicating signal. It costs nothing
// and helps in edge cases, such as Russian task titles inside an English
// prompt.
//
// IMPORTANT: prompts that work on note text (summarize, rewrite, shorten, fix
// grammar) are not covered here — they must answer in the language of the text
// itself. Changing the language of someone's note at the press of "shorten" is
// a bug, not localization. Those calls go through ask_ai_verbatim.
fn with_lang(system: &str, lang: crate::i18n::Lang) -> String {
    let req = match lang {
        crate::i18n::Lang::Ru => "Отвечай на русском языке.",
        crate::i18n::Lang::En => "Reply in English.",
    };
    format!("{system}\n{req}")
}

/// A Russian/English prompt pair for a single action.
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

/// Ask the model without forcing the answer's language. For prompts where the
/// language is set by the request's content (the note text), not the interface.
pub async fn ask_ai_verbatim(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    ask_ai_inner(app, system.to_string(), user).await
}

/// Ask the model in the interface language: the prompt of the right language is
/// selected, plus the duplicating requirement at the end.
pub async fn ask_ai_localized(app: &tauri::AppHandle, prompt: &Prompt, user: &str) -> Result<String, String> {
    let lang = crate::i18n::current_lang(app.state::<SqlitePool>().inner()).await;
    ask_ai_inner(app, with_lang(prompt.pick(lang), lang), user).await
}

/// A prompt with no language pair (machine JSON, where the text is the same for
/// every language) but with the language requirement — a human reads the answer.
pub async fn ask_ai(app: &tauri::AppHandle, system: &str, user: &str) -> Result<String, String> {
    let lang = crate::i18n::current_lang(app.state::<SqlitePool>().inner()).await;
    ask_ai_inner(app, with_lang(system, lang), user).await
}

async fn ask_ai_inner(app: &tauri::AppHandle, system: String, user: &str) -> Result<String, String> {
    let system: &str = &system;
    let settings = crate::commands::settings::load_settings_raw(app.state::<SqlitePool>().inner())
        .await
        .map_err(|e| e.to_string())?;

    // AI explicitly turned off: do not start the local model or call the cloud
    if settings.ai_provider == "none" {
        return Err("ИИ отключён в настройках".into());
    }

    if !settings.ai_fallback {
        // The former behaviour: a single provider, no fallback
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
        // Categories are user-defined: the prompt is built from the table, the
        // model's answer is matched against it, and a category id goes out.
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

// AI classification of unknown applications.
//
// Apps with no matching rule all land in "Other", and writing globs by hand is
// exactly the kind of work worth handing to a model: it knows that "jetbrains-idea"
// is a development environment and "steam_app_123" is a game, whereas the app has
// no way to tell.
//
// Suggest-then-confirm, like every other AI feature here: the model only proposes,
// and the rules are created by an explicit click. Writing them straight into the
// settings would silently rewrite statistics for past days — the categories are
// applied at read time, so a wrong rule would retroactively distort the dashboard.
//
// A ready glob is requested rather than a bare category: one "jetbrains-*" rule
// covers a whole family, while a rule per exact name would accumulate one entry per
// application.
const SYSTEM_APP_RULES: &str = "\
Ты классифицируешь приложения по категориям использования времени.
Категории: Work, Study, Home, Health.
Ответь ТОЛЬКО JSON-массивом без пояснений, каждый элемент вида
{\"pattern\": \"...\", \"category\": \"...\"}.
pattern — маска класса окна, можно с '*' для семейства приложений
(например jetbrains-idea -> \"jetbrains-*\").
category — ровно одно слово из списка выше.
Если приложение непонятно, просто пропусти его.";

#[derive(Clone, Serialize)]
pub struct AppRulePayload {
    pub rules: Option<Vec<crate::commands::monitor::CategoryRule>>,
    pub error: Option<String>,
}

// A strict parse of the model's answer into rules. We do not trust the model, in the
// same spirit as parse_subtasks: at the slightest doubt an item is dropped rather
// than guessed at.
//
// Everything is checked against reality rather than against the JSON's shape alone:
// the category must be one of the known ones (an invented one would silently become
// "Other" in categorize_app, so a rule with it does nothing at all), and the pattern
// must actually match the application it was proposed for — a model that hallucinates
// "jetbrains-*" for "firefox" would otherwise produce a rule that quietly
// miscategorizes something else.
pub fn parse_app_rules(raw: &str, apps: &[String]) -> Vec<crate::commands::monitor::CategoryRule> {
    let start = match raw.find('[') { Some(i) => i, None => return Vec::new() };
    let end = match raw.rfind(']') { Some(i) => i, None => return Vec::new() };
    if end <= start { return Vec::new(); }

    let parsed: Vec<crate::commands::monitor::CategoryRule> =
        match serde_json::from_str(&raw[start..=end]) { Ok(v) => v, Err(_) => return Vec::new() };

    let mut out: Vec<crate::commands::monitor::CategoryRule> = Vec::new();
    for rule in parsed {
        let pattern = rule.pattern.trim().to_string();
        let category = rule.category.trim().to_string();
        if pattern.is_empty() || !crate::commands::monitor::is_known_category(&category) {
            continue;
        }
        // "*" alone would swallow every application at once.
        if pattern.chars().all(|c| c == '*') {
            continue;
        }
        if !apps.iter().any(|a| crate::commands::monitor::glob_match(&pattern, a)) {
            continue;
        }
        if out.iter().any(|r: &crate::commands::monitor::CategoryRule| r.pattern == pattern) {
            continue;
        }
        out.push(crate::commands::monitor::CategoryRule { pattern, category });
    }
    out
}

#[tauri::command]
pub async fn ai_suggest_app_rules(
    app: tauri::AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
) -> Result<(), String> {
    let pool = pool.inner().clone();
    tokio::spawn(async move {
        let r = async {
            let apps = crate::commands::monitor::uncategorized_apps_impl(&pool, 30, 15)
                .await
                .map_err(|e| e.to_string())?;
            if apps.is_empty() {
                return Ok(Vec::new());
            }
            let names: Vec<String> = apps.iter().map(|a| a.app.clone()).collect();
            let listing = names.join("\n");
            // verbatim: the answer is machine JSON of window classes and category
            // ids, with not one word for a human. A language requirement here would
            // only raise the chance of prose instead of an array.
            let answer = ask_ai_verbatim(&app, SYSTEM_APP_RULES, &listing).await?;
            Ok(parse_app_rules(&answer, &names))
        }.await;

        let payload = match r {
            Ok(rules) => AppRulePayload { rules: Some(rules), error: None },
            Err(e) => AppRulePayload { rules: None, error: Some(e) },
        };
        let _ = app.emit("ai-app-rules", payload);
    });
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct InsightPayload {
    pub result: Option<String>,
    pub error: Option<String>,
}

// A short activity digest for the last few days — the input for the AI insight.
//
// The language of the context must match the language of the prompt. An English
// instruction on top of Russian context does not work: the model sees a wall of
// Russian data (activity minutes per day, task titles, categories) and answers
// in that language. Verified by the user against a local model, where the
// insight and the digest stayed Russian even though the short SMART prompt,
// which carries no context, translated fine.
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

// Period data for the digest: completed tasks, active minutes, overdue items.
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

    // Weekly review: project goals and top apps — the data from phases 1-2
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
        // the original prompt is not lost
        assert!(ru.starts_with("Ты ассистент."));
        assert!(en.starts_with("Ты ассистент."));
        // and the languages are not swapped
        assert!(!en.contains("Отвечай на русском"));
        assert!(!ru.contains("Reply in English"));
    }

    // verbatim prompts must say nothing about language: theirs is set by the
    // request's content (the note text), or it does not matter at all (machine
    // JSON). A "in Russian" requirement here would break editing an English
    // note.
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
                // _RU/_EN pairs are declared as `Prompt {` — they set the
                // language on purpose, so this check is not about them
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

    // The central invariant: both sides of every pair are filled in and really
    // differ. Copying the Russian text into the en field (or forgetting to fill
    // it) is the likeliest way to quietly bring the old bug back: an English
    // interface with a Russian answer.
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
            // the Russian side must be Russian, the English one free of Cyrillic
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

    // Declaring the pairs is not enough — they must also be called through
    // ask_ai_localized. Reaching for `.ru`/`.en` at the call site nails the
    // language down, bypassing the setting, and that is exactly the bug this
    // guards against: the declared pair still looks correct while the user gets
    // one language anyway. The check reads the sources because the calls are
    // async and need a live AppHandle.
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
                // a pair declaration (`ru:`/`en:` inside Prompt {}), not a call
                if frag.contains("Prompt") {
                    continue;
                }
                for pinned in [".ru", ".en"] {
                    // in tests reaching for the fields is legitimate: they compare pairs
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
        // Primary openai: cloud first, then the second key, then local
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
        // No local model and no anthropic key
        assert_eq!(
            resolve_provider_order("openai", false, true, false),
            vec![Provider::OpenAi]
        );
        // Primary without a key: fall back to an available one straight away
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

    // The central invariant: the context sent into the prompt must be in the
    // interface language. An English instruction on top of Russian context does
    // not work — verified by the user against a local model: the insight and the
    // digest stayed Russian even though the short SMART prompt, which has no
    // context, translated fine. The model follows the language of the data, not
    // of the instruction.
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

        // days >= 7 enables the project-goals and top-apps branches
        let period = period_summary(&pool, 7, "последняя неделя", crate::i18n::Lang::En)
            .await
            .unwrap();
        assert!(!has_cyrillic(&period), "кириллица в английской сводке: {period}");
        // the period label is translated too, not left as a key
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
        insert_completed_task(&pool, "старая", &old).await; // outside the period
        insert_activity(&pool, &recent, "Active", 300).await;
        insert_activity(&pool, &recent, "Idle", 3600).await; // Idle is not counted
        insert_activity(&pool, &old, "Active", 3600).await; // outside the period

        let s = period_summary(&pool, 7, "неделя", crate::i18n::Lang::Ru).await.unwrap();
        assert!(s.contains("Выполнено задач: 1 (свежая)"), "{s}");
        assert!(!s.contains("старая"), "{s}");
        assert!(s.contains("Активное время: 5 мин"), "{s}");
    }

    // --- parse_subtasks: "do not trust the model" — dirty LLM answers ---

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
        // "[1, 2, 3]" used to survive whole as a single bogus item.
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

    // --- AI classification of applications ---
    //
    // The parser is the whole safety of this feature: a suggestion becomes a rule the
    // user clicks in, and a rule silently rewrites statistics for past days (the
    // categories are applied at read time).

    fn apps() -> Vec<String> {
        vec!["jetbrains-idea".to_string(), "firefox".to_string(), "steam_app_570".to_string()]
    }

    #[test]
    fn app_rules_parse_from_json_array() {
        let raw = r#"[{"pattern":"jetbrains-*","category":"Work"},{"pattern":"firefox","category":"Study"}]"#;
        let rules = parse_app_rules(raw, &apps());
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "jetbrains-*");
        assert_eq!(rules[0].category, "Work");
    }

    #[test]
    fn app_rules_survive_prose_around_json() {
        // A model regularly adds an explanation despite being asked not to.
        let raw = "Вот правила:\n[{\"pattern\":\"firefox\",\"category\":\"Study\"}]\nГотово.";
        assert_eq!(parse_app_rules(raw, &apps()).len(), 1);
    }

    #[test]
    fn app_rules_reject_unknown_category() {
        // An invented category silently becomes "Other" in categorize_app, so such a
        // rule would do nothing at all while looking legitimate in the settings.
        let raw = r#"[{"pattern":"firefox","category":"Entertainment"}]"#;
        assert!(parse_app_rules(raw, &apps()).is_empty());
    }

    #[test]
    fn app_rules_reject_pattern_matching_nothing() {
        // The central guard against a hallucination: a pattern must match the very
        // application it was proposed for, or the rule would quietly categorize
        // something else entirely.
        let raw = r#"[{"pattern":"photoshop*","category":"Work"}]"#;
        assert!(parse_app_rules(raw, &apps()).is_empty());
    }

    #[test]
    fn app_rules_reject_catch_all_pattern() {
        // "*" would swallow every application at once, including those the user has
        // already categorized by hand.
        for p in ["*", "**"] {
            let raw = format!(r#"[{{"pattern":"{p}","category":"Work"}}]"#);
            assert!(parse_app_rules(&raw, &apps()).is_empty(), "pattern {p} must be rejected");
        }
    }

    #[test]
    fn app_rules_drop_duplicate_patterns() {
        let raw = r#"[{"pattern":"firefox","category":"Work"},{"pattern":"firefox","category":"Study"}]"#;
        let rules = parse_app_rules(raw, &apps());
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].category, "Work", "the first one wins, as in categorize_app");
    }

    #[test]
    fn app_rules_garbage_yields_nothing() {
        for raw in ["", "не понял задачу", "{}", "[", "[{\"pattern\":}]"] {
            assert!(parse_app_rules(raw, &apps()).is_empty(), "raw {raw:?} must yield nothing");
        }
    }

    // --- AI on a selection ---

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
