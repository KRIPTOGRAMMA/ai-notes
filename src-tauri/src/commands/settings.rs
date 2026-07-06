use std::sync::{Arc, Mutex};
use tauri::State;
use sqlx::{SqlitePool, Row};
use serde::{Deserialize, Serialize};
use crate::error::AppResult;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum WorkMode {
    #[default]
    Light,
    Study,
    Focus,
}

impl WorkMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkMode::Light => "Light",
            WorkMode::Study => "Study",
            WorkMode::Focus => "Focus",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Study" => WorkMode::Study,
            "Focus" => WorkMode::Focus,
            _ => WorkMode::Light,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub ai_provider: String,   // "local" | "openai" | "anthropic"
    pub openai_key: String,
    pub openai_model: String,
    pub anthropic_key: String,
    pub anthropic_model: String,
    pub idle_threshold_secs: u64,  // порог простоя; применяется после перезапуска
    pub log_interval_secs: u64,    // интервал тика activity-loop
    pub work_mode: WorkMode,   // Light | Study | Focus
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_provider: "local".into(),
            openai_key: String::new(),
            openai_model: "gpt-4o-mini".into(),
            anthropic_key: String::new(),
            anthropic_model: "claude-haiku-4-5-20251001".into(),
            idle_threshold_secs: 300,
            log_interval_secs: 60,
            work_mode: WorkMode::Light,
        }
    }
}

// API-ключи храним в системном keyring (Secret Service / Windows Credential
// Manager), а не в SQLite открытым текстом. Если keyring недоступен
// (нет демона) — падаем обратно на таблицу settings.
fn keyring_set(name: &str, value: &str) -> Result<(), keyring::Error> {
    let entry = keyring::Entry::new("ai-notes", name)?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        entry.set_password(value)
    }
}

fn keyring_get(name: &str) -> Option<String> {
    keyring::Entry::new("ai-notes", name).ok()?.get_password().ok()
}

async fn get_setting(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| r.get("value"))
}

async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

// Для переключения режима из трея: пишем в БД мимо полного save_settings
pub async fn persist_work_mode(pool: &SqlitePool, mode: &WorkMode) -> AppResult<()> {
    set_setting(pool, "work_mode", mode.as_str()).await
}

pub async fn load_settings_raw(pool: &SqlitePool) -> AppResult<AppSettings> {
    let mut s = AppSettings::default();
    if let Some(v) = get_setting(pool, "ai_provider").await { s.ai_provider = v; }
    if let Some(v) = get_setting(pool, "openai_model").await { s.openai_model = v; }
    if let Some(v) = get_setting(pool, "anthropic_model").await { s.anthropic_model = v; }
    if let Some(v) = get_setting(pool, "idle_threshold_secs").await {
        if let Ok(n) = v.parse() { s.idle_threshold_secs = n; }
    }
    if let Some(v) = get_setting(pool, "log_interval_secs").await {
        if let Ok(n) = v.parse() { s.log_interval_secs = n; }
    }
    if let Some(v) = get_setting(pool, "work_mode").await {
        s.work_mode = WorkMode::from_str(&v);
    }
    // Ключи: сначала keyring, затем legacy-значение из БД
    s.openai_key = keyring_get("openai_key")
        .or(get_setting(pool, "openai_key").await)
        .unwrap_or_default();
    s.anthropic_key = keyring_get("anthropic_key")
        .or(get_setting(pool, "anthropic_key").await)
        .unwrap_or_default();
    Ok(s)
}

#[tauri::command]
pub async fn get_settings(pool: State<'_, SqlitePool>) -> AppResult<AppSettings> {
    load_settings_raw(pool.inner()).await
}

#[tauri::command]
pub async fn save_settings(
    pool: State<'_, SqlitePool>,
    mode_state: State<'_, Arc<Mutex<WorkMode>>>,
    settings: AppSettings,
) -> AppResult<()> {
    set_setting(pool.inner(), "ai_provider", &settings.ai_provider).await?;
    set_setting(pool.inner(), "openai_model", &settings.openai_model).await?;
    set_setting(pool.inner(), "anthropic_model", &settings.anthropic_model).await?;
    // Минимумы: не даём выставить значения, ломающие трекинг
    set_setting(pool.inner(), "idle_threshold_secs", &settings.idle_threshold_secs.max(60).to_string()).await?;
    set_setting(pool.inner(), "log_interval_secs", &settings.log_interval_secs.clamp(10, 600).to_string()).await?;
    set_setting(pool.inner(), "work_mode", settings.work_mode.as_str()).await?;

    for (name, value) in [("openai_key", &settings.openai_key), ("anthropic_key", &settings.anthropic_key)] {
        match keyring_set(name, value) {
            Ok(()) => {
                // Ключ в keyring — подчищаем возможную legacy-копию в БД
                set_setting(pool.inner(), name, "").await?;
            }
            Err(_) => {
                // Keyring недоступен — fallback на БД (как раньше)
                set_setting(pool.inner(), name, value).await?;
            }
        }
    }

    *mode_state.lock().unwrap() = settings.work_mode.clone();
    Ok(())
}
