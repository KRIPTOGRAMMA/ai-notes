use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use crate::commands::settings::{WorkMode, get_setting};

// Единый гейт всех уведомлений: режим Focus ИЛИ активная пауза quiet_until.
// Чистая функция — вся асинхронщина снаружи.
pub fn notifications_muted(
    mode: &WorkMode,
    quiet_until: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    *mode == WorkMode::Focus || quiet_until.map(|t| now < t).unwrap_or(false)
}

pub async fn quiet_until(pool: &SqlitePool) -> Option<DateTime<Utc>> {
    let v = get_setting(pool, "quiet_until").await?;
    DateTime::parse_from_rfc3339(&v).ok().map(|t| t.with_timezone(&Utc))
}

// Комбинированный хелпер для точек отправки: читает паузу из БД и применяет гейт.
pub async fn muted_now(pool: &SqlitePool, mode: &WorkMode) -> bool {
    notifications_muted(mode, quiet_until(pool).await, Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        "2026-07-10T12:00:00+00:00".parse().unwrap()
    }

    #[test]
    fn focus_always_mutes() {
        assert!(notifications_muted(&WorkMode::Focus, None, now()));
        assert!(notifications_muted(&WorkMode::Focus, Some(now() - Duration::hours(1)), now()));
    }

    #[test]
    fn active_pause_mutes_in_any_mode() {
        let until = Some(now() + Duration::minutes(30));
        assert!(notifications_muted(&WorkMode::Light, until, now()));
        assert!(notifications_muted(&WorkMode::Study, until, now()));
    }

    #[test]
    fn expired_pause_does_not_mute() {
        let until = Some(now() - Duration::seconds(1));
        assert!(!notifications_muted(&WorkMode::Light, until, now()));
    }

    #[test]
    fn no_pause_light_not_muted() {
        assert!(!notifications_muted(&WorkMode::Light, None, now()));
    }

    #[test]
    fn forever_sentinel_mutes() {
        let until = DateTime::parse_from_rfc3339(crate::commands::settings::QUIET_FOREVER)
            .map(|t| t.with_timezone(&Utc))
            .ok();
        assert!(notifications_muted(&WorkMode::Light, until, now()));
    }
}
