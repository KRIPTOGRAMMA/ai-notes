use tauri::State;
use sqlx::{SqlitePool, Row};
use serde::{Deserialize, Serialize};
use crate::error::AppResult;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub ai_provider: String,   // "local" | "openai" | "anthropic"
    pub openai_key: String,
    pub openai_model: String,
    pub anthropic_key: String,
    pub anthropic_model: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_provider: "local".into(),
            openai_key: String::new(),
            openai_model: "gpt-4o-mini".into(),
            anthropic_key: String::new(),
            anthropic_model: "claude-haiku-4-5-20251001".into(),
        }
    }
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

pub async fn load_settings_raw(pool: &SqlitePool) -> AppResult<AppSettings> {
    let mut s = AppSettings::default();
    if let Some(v) = get_setting(pool, "ai_provider").await { s.ai_provider = v; }
    if let Some(v) = get_setting(pool, "openai_key").await { s.openai_key = v; }
    if let Some(v) = get_setting(pool, "openai_model").await { s.openai_model = v; }
    if let Some(v) = get_setting(pool, "anthropic_key").await { s.anthropic_key = v; }
    if let Some(v) = get_setting(pool, "anthropic_model").await { s.anthropic_model = v; }
    Ok(s)
}

#[tauri::command]
pub async fn get_settings(pool: State<'_, SqlitePool>) -> AppResult<AppSettings> {
    load_settings_raw(pool.inner()).await
}

#[tauri::command]
pub async fn save_settings(pool: State<'_, SqlitePool>, settings: AppSettings) -> AppResult<()> {
    set_setting(pool.inner(), "ai_provider", &settings.ai_provider).await?;
    set_setting(pool.inner(), "openai_key", &settings.openai_key).await?;
    set_setting(pool.inner(), "openai_model", &settings.openai_model).await?;
    set_setting(pool.inner(), "anthropic_key", &settings.anthropic_key).await?;
    set_setting(pool.inner(), "anthropic_model", &settings.anthropic_model).await?;
    Ok(())
}
