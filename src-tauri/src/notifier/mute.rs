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

// Фокус-режим (v0.9.12): авто-пауза на время помодоро-работы / активного
// тайм-блока. Никогда не укорачивает уже действующую паузу — только продлевает
// (пользователь мог вручную поставить "бессрочно" или более далёкий таймер).
pub async fn extend_quiet_until(pool: &SqlitePool, until: DateTime<Utc>) {
    let current = quiet_until(pool).await;
    if current.map(|t| t >= until).unwrap_or(false) {
        return;
    }
    crate::commands::settings::persist_quiet_until(pool, &until.to_rfc3339()).await.ok();
}

#[cfg(test)]
mod extend_tests {
    use super::*;
    use chrono::Duration;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn extends_when_no_existing_pause() {
        let pool = test_pool().await;
        let until = Utc::now() + Duration::minutes(25);
        extend_quiet_until(&pool, until).await;
        assert_eq!(quiet_until(&pool).await, Some(until));
    }

    #[tokio::test]
    async fn does_not_shorten_a_longer_existing_pause() {
        let pool = test_pool().await;
        let far = Utc::now() + Duration::hours(2);
        crate::commands::settings::persist_quiet_until(&pool, &far.to_rfc3339()).await.unwrap();
        let near = Utc::now() + Duration::minutes(10);
        extend_quiet_until(&pool, near).await;
        assert_eq!(quiet_until(&pool).await, Some(far));
    }

    #[tokio::test]
    async fn extends_a_shorter_existing_pause() {
        let pool = test_pool().await;
        let near = Utc::now() + Duration::minutes(5);
        crate::commands::settings::persist_quiet_until(&pool, &near.to_rfc3339()).await.unwrap();
        let far = Utc::now() + Duration::minutes(25);
        extend_quiet_until(&pool, far).await;
        assert_eq!(quiet_until(&pool).await, Some(far));
    }
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
