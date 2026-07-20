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
    pub ai_provider: String,   // "none" | "local" | "openai" | "anthropic"
    pub openai_key: String,
    pub openai_model: String,
    pub anthropic_key: String,
    pub anthropic_model: String,
    pub idle_threshold_secs: u64,  // порог простоя; применяется после перезапуска
    pub log_interval_secs: u64,    // интервал тика activity-loop
    pub work_mode: WorkMode,   // Light | Study | Focus
    pub onboarding_complete: bool,
    pub deadline_warn_hours: u64,    // за сколько часов до дедлайна первое уведомление
    pub deadline_warn_minutes: u64,  // за сколько минут до дедлайна второе уведомление
    pub idle_notify_min_mins: u64,   // минимальный простой (минуты) для notify_return
    pub pomodoro_work_mins: u64,     // длина рабочего блока помодоро
    pub pomodoro_break_mins: u64,    // длина перерыва помодоро
    pub nudge_after_mins: u64,       // напоминание о перерыве после N мин непрерывной работы (0 — выкл)
    #[serde(default)]
    pub theme_mode: String,          // "light" | "dark" | "system"
    #[serde(default)]
    pub color_accent: String,        // оверрайды цветов; пусто = дефолт из CSS
    #[serde(default)]
    pub color_bg: String,
    #[serde(default)]
    pub color_text: String,
    #[serde(default)]
    pub color_border: String,
    #[serde(default)]
    pub quiet_until: String,         // пауза уведомлений: RFC3339; пусто = выкл; QUIET_FOREVER = бессрочно
    #[serde(default = "default_true")]
    pub context_notifications: bool, // контекстные триггеры (просрочки, возврат с InProgress, пропуски дней)
    #[serde(default)]
    pub ai_fallback: bool,           // автопереключение ИИ-провайдера при ошибке/недоступности
    #[serde(default)]
    pub openai_in_keyring: bool,     // runtime-only: ключ хранится в keyring
    #[serde(default)]
    pub anthropic_in_keyring: bool,  // runtime-only: ключ хранится в keyring
    #[serde(default)]
    pub app_category_rules: String,  // JSON [{pattern, category}] — классы окон → категории
    #[serde(default)]
    pub app_limits: String,          // JSON [{category, daily_mins}] — 0/отсутствие = без лимита
    #[serde(default)]
    pub auto_backup_dir: String,     // пусто = авто-бэкап выключен
    #[serde(default = "default_seven")]
    pub auto_backup_keep: u64,       // сколько копий хранить
    #[serde(default)]
    pub morning_digest_time: String, // "HH:MM", пусто = выкл
    #[serde(default = "default_true")]
    pub show_subtasks_expanded: bool, // v0.8.3: подзадачи в списке видны без клика
}

fn default_seven() -> u64 { 7 }

fn default_true() -> bool { true }

// Сентинел «бессрочной» паузы уведомлений.
pub const QUIET_FOREVER: &str = "9999-12-31T00:00:00+00:00";

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
            onboarding_complete: false,
            deadline_warn_hours: 24,
            deadline_warn_minutes: 60,
            idle_notify_min_mins: 10,
            pomodoro_work_mins: 25,
            pomodoro_break_mins: 5,
            nudge_after_mins: 90,
            theme_mode: "system".into(),
            color_accent: String::new(),
            color_bg: String::new(),
            color_text: String::new(),
            color_border: String::new(),
            quiet_until: String::new(),
            context_notifications: true,
            ai_fallback: false,
            openai_in_keyring: false,
            anthropic_in_keyring: false,
            app_category_rules: String::new(),
            app_limits: String::new(),
            auto_backup_dir: String::new(),
            auto_backup_keep: 7,
            morning_digest_time: String::new(),
            show_subtasks_expanded: true,
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

pub(crate) async fn get_setting(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|r| r.get("value"))
}

// Единая точка чтения числовой настройки: используется фоновыми циклами
// (scheduler / pomodoro / activity), чтобы не плодить копии одного и того же
// запроса. Отсутствие ключа или мусор в значении → default.
pub async fn get_u64_setting(pool: &SqlitePool, key: &str, default: u64) -> u64 {
    get_setting(pool, key).await.and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub(crate) async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
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

// Для паузы уведомлений из трея: пустая строка = пауза снята.
pub async fn persist_quiet_until(pool: &SqlitePool, value: &str) -> AppResult<()> {
    set_setting(pool, "quiet_until", value).await
}

// Какой пресет паузы выбран (id пункта трея: quiet_30/quiet_60/...) — чтобы
// после перезапуска восстановить галочку таймерной паузы в трее.
pub async fn persist_quiet_preset(pool: &SqlitePool, id: &str) -> AppResult<()> {
    set_setting(pool, "quiet_preset", id).await
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
    if let Some(v) = get_setting(pool, "onboarding_complete").await {
        s.onboarding_complete = v == "true";
    }
    if let Some(v) = get_setting(pool, "deadline_warn_hours").await { if let Ok(n) = v.parse() { s.deadline_warn_hours = n; } }
    if let Some(v) = get_setting(pool, "deadline_warn_minutes").await { if let Ok(n) = v.parse() { s.deadline_warn_minutes = n; } }
    if let Some(v) = get_setting(pool, "idle_notify_min_mins").await { if let Ok(n) = v.parse() { s.idle_notify_min_mins = n; } }
    if let Some(v) = get_setting(pool, "pomodoro_work_mins").await { if let Ok(n) = v.parse() { s.pomodoro_work_mins = n; } }
    if let Some(v) = get_setting(pool, "pomodoro_break_mins").await { if let Ok(n) = v.parse() { s.pomodoro_break_mins = n; } }
    if let Some(v) = get_setting(pool, "nudge_after_mins").await { if let Ok(n) = v.parse() { s.nudge_after_mins = n; } }
    if let Some(v) = get_setting(pool, "theme_mode").await { s.theme_mode = v; }
    if let Some(v) = get_setting(pool, "color_accent").await { s.color_accent = v; }
    if let Some(v) = get_setting(pool, "color_bg").await { s.color_bg = v; }
    if let Some(v) = get_setting(pool, "color_text").await { s.color_text = v; }
    if let Some(v) = get_setting(pool, "color_border").await { s.color_border = v; }
    if let Some(v) = get_setting(pool, "quiet_until").await { s.quiet_until = v; }
    if let Some(v) = get_setting(pool, "context_notifications").await { s.context_notifications = v != "false"; }
    if let Some(v) = get_setting(pool, "ai_fallback").await { s.ai_fallback = v == "true"; }
    if let Some(v) = get_setting(pool, "app_category_rules").await { s.app_category_rules = v; }
    if let Some(v) = get_setting(pool, "app_limits").await { s.app_limits = v; }
    if let Some(v) = get_setting(pool, "auto_backup_dir").await { s.auto_backup_dir = v; }
    if let Some(v) = get_setting(pool, "auto_backup_keep").await {
        if let Ok(n) = v.parse() { s.auto_backup_keep = n; }
    }
    if let Some(v) = get_setting(pool, "morning_digest_time").await { s.morning_digest_time = v; }
    if let Some(v) = get_setting(pool, "show_subtasks_expanded").await { s.show_subtasks_expanded = v != "false"; }
    // Ключи: сначала keyring, затем legacy-значение из БД
    let openai_from_keyring = keyring_get("openai_key");
    let anthropic_from_keyring = keyring_get("anthropic_key");
    s.openai_in_keyring = openai_from_keyring.is_some();
    s.anthropic_in_keyring = anthropic_from_keyring.is_some();
    s.openai_key = openai_from_keyring
        .or(get_setting(pool, "openai_key").await)
        .unwrap_or_default();
    s.anthropic_key = anthropic_from_keyring
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
    app: tauri::AppHandle,
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
    set_setting(pool.inner(), "onboarding_complete", if settings.onboarding_complete { "true" } else { "false" }).await?;
    set_setting(pool.inner(), "deadline_warn_hours", &settings.deadline_warn_hours.max(1).to_string()).await?;
    set_setting(pool.inner(), "deadline_warn_minutes", &settings.deadline_warn_minutes.clamp(1, 1440).to_string()).await?;
    set_setting(pool.inner(), "idle_notify_min_mins", &settings.idle_notify_min_mins.max(1).to_string()).await?;
    set_setting(pool.inner(), "pomodoro_work_mins", &settings.pomodoro_work_mins.clamp(1, 120).to_string()).await?;
    set_setting(pool.inner(), "pomodoro_break_mins", &settings.pomodoro_break_mins.clamp(1, 60).to_string()).await?;
    // 0 = выключено; иначе минимум 20 минут, чтобы не спамить
    set_setting(pool.inner(), "nudge_after_mins", &(if settings.nudge_after_mins == 0 { 0 } else { settings.nudge_after_mins.max(20) }).to_string()).await?;
    // Тема: режим + цветовые оверрайды (пустая строка = дефолт из CSS)
    let theme_mode = match settings.theme_mode.as_str() { "light" | "dark" | "system" => settings.theme_mode.as_str(), _ => "system" };
    set_setting(pool.inner(), "theme_mode", theme_mode).await?;
    set_setting(pool.inner(), "color_accent", &settings.color_accent).await?;
    set_setting(pool.inner(), "color_bg", &settings.color_bg).await?;
    set_setting(pool.inner(), "color_text", &settings.color_text).await?;
    set_setting(pool.inner(), "color_border", &settings.color_border).await?;
    // Пауза уведомлений: пусто = выкл; иначе только валидный RFC3339
    let quiet = if settings.quiet_until.is_empty()
        || chrono::DateTime::parse_from_rfc3339(&settings.quiet_until).is_ok()
    {
        settings.quiet_until.as_str()
    } else {
        ""
    };
    set_setting(pool.inner(), "quiet_until", quiet).await?;
    set_setting(pool.inner(), "context_notifications", if settings.context_notifications { "true" } else { "false" }).await?;
    set_setting(pool.inner(), "ai_fallback", if settings.ai_fallback { "true" } else { "false" }).await?;
    // Правила категоризации приложений: храним только валидный JSON-массив
    let rules = if crate::commands::monitor::parse_category_rules(&settings.app_category_rules).is_empty()
        && !settings.app_category_rules.trim().is_empty()
    {
        "" // мусор не сохраняем
    } else {
        settings.app_category_rules.as_str()
    };
    set_setting(pool.inner(), "app_category_rules", rules).await?;
    // Лимиты категорий: та же логика — мусор не сохраняем
    let limits = if crate::commands::monitor::parse_app_limits(&settings.app_limits).is_empty()
        && !settings.app_limits.trim().is_empty()
    {
        ""
    } else {
        settings.app_limits.as_str()
    };
    set_setting(pool.inner(), "app_limits", limits).await?;
    set_setting(pool.inner(), "auto_backup_dir", &settings.auto_backup_dir).await?;
    set_setting(pool.inner(), "auto_backup_keep", &settings.auto_backup_keep.max(1).to_string()).await?;
    set_setting(pool.inner(), "morning_digest_time", &settings.morning_digest_time).await?;
    set_setting(pool.inner(), "show_subtasks_expanded", if settings.show_subtasks_expanded { "true" } else { "false" }).await?;

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
    crate::update_mode_checks(&app, &settings.work_mode);
    Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn work_mode_roundtrip() {
    for mode in [WorkMode::Light, WorkMode::Study, WorkMode::Focus] {
      assert_eq!(WorkMode::from_str(mode.as_str()), mode);
    }
  }

  #[test]
  fn work_mode_unknown_falls_back_to_light() {
    assert_eq!(WorkMode::from_str("abrakadabra"), WorkMode::Light);
    assert_eq!(WorkMode::from_str(""), WorkMode::Light);
  }
}
// Интеграционные тесты на in-memory SQLite: реальные миграции, реальные запросы
#[cfg(test)]
mod db_tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn defaults_when_db_empty() {
        let pool = test_pool().await;
        let s = load_settings_raw(&pool).await.unwrap();
        assert_eq!(s.work_mode, WorkMode::Light);
        assert_eq!(s.idle_threshold_secs, 300);
        assert!(!s.onboarding_complete);
    }

    #[tokio::test]
    async fn set_get_roundtrip() {
        let pool = test_pool().await;
        set_setting(&pool, "ai_provider", "anthropic").await.unwrap();
        assert_eq!(get_setting(&pool, "ai_provider").await.unwrap(), "anthropic");

        // Повторная запись перезаписывает, а не дублирует
        set_setting(&pool, "ai_provider", "openai").await.unwrap();
        assert_eq!(get_setting(&pool, "ai_provider").await.unwrap(), "openai");
    }

    #[tokio::test]
    async fn persist_work_mode_is_loaded_back() {
        let pool = test_pool().await;
        persist_work_mode(&pool, &WorkMode::Focus).await.unwrap();
        let s = load_settings_raw(&pool).await.unwrap();
        assert_eq!(s.work_mode, WorkMode::Focus);
    }
}
