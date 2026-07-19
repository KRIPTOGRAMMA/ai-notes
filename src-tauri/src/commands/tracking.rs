use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::State;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveSession {
    pub task_id: String,
    pub title: String,
    pub started_at: String,
    pub elapsed_secs: i64,
}

/// Закрыть любую открытую сессию, открыть новую для task_id, задачу → InProgress.
#[tauri::command]
pub async fn start_task_tracking(
    pool: State<'_, SqlitePool>,
    task_id: String,
) -> AppResult<ActiveSession> {
    start_task_tracking_impl(pool.inner(), &task_id).await
}

pub async fn start_task_tracking_impl(pool: &SqlitePool, task_id: &str) -> AppResult<ActiveSession> {
    // 1. Закрыть любую открытую сессию
    sqlx::query("UPDATE task_sessions SET ended_at = ? WHERE ended_at IS NULL")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;

    // 2. Проверить, что задача существует
    let title: String = sqlx::query_scalar("SELECT title FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Other("Задача не найдена".into()))?;

    // 3. Открыть новую сессию
    let now = Utc::now();
    let session = ActiveSession {
        task_id: task_id.to_string(),
        title: title.clone(),
        started_at: now.to_rfc3339(),
        elapsed_secs: 0,
    };
    sqlx::query(
        "INSERT INTO task_sessions (id, task_id, started_at, ended_at) VALUES (?, ?, ?, NULL)"
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(task_id)
    .bind(&session.started_at)
    .execute(pool)
    .await?;

    // 4. Задачу → InProgress
    sqlx::query("UPDATE tasks SET status = 'InProgress', updated_at = ? WHERE id = ?")
        .bind(&now.to_rfc3339())
        .bind(task_id)
        .execute(pool)
        .await?;

    Ok(session)
}

/// Закрыть открытую сессию (если есть).
#[tauri::command]
pub async fn stop_task_tracking(pool: State<'_, SqlitePool>) -> AppResult<()> {
    stop_task_tracking_impl(pool.inner()).await
}

pub async fn stop_task_tracking_impl(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("UPDATE task_sessions SET ended_at = ? WHERE ended_at IS NULL")
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Вернуть активную сессию + прошедшие секунды.
#[tauri::command]
pub async fn get_active_session(pool: State<'_, SqlitePool>) -> AppResult<Option<ActiveSession>> {
    get_active_session_impl(pool.inner()).await
}

pub async fn get_active_session_impl(pool: &SqlitePool) -> AppResult<Option<ActiveSession>> {
    let now = Utc::now();
    let row = sqlx::query(
        "SELECT s.task_id, s.started_at, t.title
         FROM task_sessions s
         JOIN tasks t ON t.id = s.task_id
         WHERE s.ended_at IS NULL
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let started: String = r.get("started_at");
        let started_dt = DateTime::parse_from_rfc3339(&started)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or(now);
        let elapsed = (now - started_dt).num_seconds().max(0);
        ActiveSession {
            task_id: r.get("task_id"),
            title: r.get("title"),
            started_at: started,
            elapsed_secs: elapsed,
        }
    }))
}

/// Сумма секунд по всем сессиям задачи (закрытые + открытая до now).
#[tauri::command]
pub async fn get_task_seconds(pool: State<'_, SqlitePool>, task_id: String) -> AppResult<i64> {
    get_task_seconds_impl(pool.inner(), &task_id).await
}

pub async fn get_task_seconds_impl(pool: &SqlitePool, task_id: &str) -> AppResult<i64> {
    let now = Utc::now();
    let rows = sqlx::query(
        "SELECT started_at, ended_at FROM task_sessions WHERE task_id = ?"
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;

    let total: i64 = rows.iter().filter_map(|r| {
        let start = DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at")).ok()?;
        let start = start.with_timezone(&Utc);
        let end: Option<DateTime<Utc>> = r.get::<Option<String>, _>("ended_at")
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc));
        let end = end.unwrap_or(now);
        Some((end - start).num_seconds().max(0))
    }).sum();

    Ok(total)
}

/// Сумма секунд по всем задачам проекта с даты from.
#[tauri::command]
pub async fn get_project_seconds(
    pool: State<'_, SqlitePool>,
    project_id: String,
    from: String,
) -> AppResult<i64> {
    get_project_seconds_impl(pool.inner(), &project_id, &from).await
}

pub async fn get_project_seconds_impl(pool: &SqlitePool, project_id: &str, from: &str) -> AppResult<i64> {
    let now = Utc::now();
    let from_dt = DateTime::parse_from_rfc3339(from)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now);

    let rows = sqlx::query(
        "SELECT s.started_at, s.ended_at
         FROM task_sessions s
         JOIN tasks t ON t.id = s.task_id
         WHERE t.project_id = ? AND s.started_at >= ?"
    )
    .bind(project_id)
    .bind(from_dt.to_rfc3339())
    .fetch_all(pool)
    .await?;

    let total: i64 = rows.iter().filter_map(|r| {
        let start = DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at")).ok()?;
        let start = start.with_timezone(&Utc);
        let end: Option<DateTime<Utc>> = r.get::<Option<String>, _>("ended_at")
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc));
        let end = end.unwrap_or(now);
        Some((end - start).num_seconds().max(0))
    }).sum();

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_task(pool: &SqlitePool, id: &str, title: &str, status: &str, project_id: Option<&str>) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(id).bind(title).bind(status)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(pool).await.unwrap();

        if let Some(pid) = project_id {
            sqlx::query("UPDATE tasks SET project_id = ? WHERE id = ?")
                .bind(pid).bind(id).execute(pool).await.unwrap();
        }
    }

    fn secs(s: i64) -> chrono::Duration {
        chrono::Duration::seconds(s)
    }

    #[tokio::test]
    async fn start_stop_roundtrip() {
        let pool = test_pool().await;
        insert_task(&pool, "t1", "Тест", "Todo", None).await;

        let s = start_task_tracking_impl(&pool, "t1").await.unwrap();
        assert_eq!(s.task_id, "t1");
        assert_eq!(s.title, "Тест");

        // Проверяем, что задача стала InProgress
        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, "InProgress");

        // Активная сессия есть
        let active = get_active_session_impl(&pool).await.unwrap();
        assert!(active.is_some());
        assert_eq!(active.unwrap().task_id, "t1");

        // Останавливаем
        stop_task_tracking_impl(&pool).await.unwrap();
        let active = get_active_session_impl(&pool).await.unwrap();
        assert!(active.is_none());
    }

    #[tokio::test]
    async fn start_closes_previous() {
        let pool = test_pool().await;
        insert_task(&pool, "t1", "Первая", "Todo", None).await;
        insert_task(&pool, "t2", "Вторая", "Todo", None).await;

        start_task_tracking_impl(&pool, "t1").await.unwrap();
        // Маленькая пауза, чтобы разница была > 0
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        start_task_tracking_impl(&pool, "t2").await.unwrap();

        // t1 сессия закрыта
        let active = get_active_session_impl(&pool).await.unwrap().unwrap();
        assert_eq!(active.task_id, "t2");

        // t1 должна иметь ended_at (проверяем что ended_at не NULL у первой сессии)
        let ended: Option<String> = sqlx::query_scalar(
            "SELECT ended_at FROM task_sessions WHERE task_id = 't1'"
        ).fetch_optional(&pool).await.unwrap().flatten();
        assert!(ended.is_some(), "Предыдущая сессия должна быть закрыта");
    }

    #[tokio::test]
    async fn get_task_seconds_sums() {
        let pool = test_pool().await;
        insert_task(&pool, "t1", "Тест", "Todo", None).await;

        let now = Utc::now();
        // Закрытая сессия 10 минут
        sqlx::query(
            "INSERT INTO task_sessions (id, task_id, started_at, ended_at) VALUES (?, ?, ?, ?)"
        )
        .bind("s1").bind("t1")
        .bind((now - secs(600)).to_rfc3339())
        .bind((now - secs(300)).to_rfc3339())
        .execute(&pool).await.unwrap();

        // Открытая сессия 5 минут
        sqlx::query(
            "INSERT INTO task_sessions (id, task_id, started_at, ended_at) VALUES (?, ?, ?, NULL)"
        )
        .bind("s2").bind("t1")
        .bind((now - secs(300)).to_rfc3339())
        .execute(&pool).await.unwrap();

        // 300 + 300 = 600 (но открытая считается до now, плюс-минус)
        let total = get_task_seconds_impl(&pool, "t1").await.unwrap();
        assert!(total >= 590 && total <= 610, "total={total}");
    }

    #[tokio::test]
    async fn get_project_seconds_sums_for_project() {
        let pool = test_pool().await;
        insert_task(&pool, "t1", "Проектная 1", "Todo", Some("p1")).await;
        insert_task(&pool, "t2", "Проектная 2", "Todo", Some("p1")).await;
        insert_task(&pool, "t3", "Без проекта", "Todo", None).await;

        let now = Utc::now();
        for (id, task_id, start_offset, end_offset) in [
            ("s1", "t1", -600, Some(-300)),
            ("s2", "t2", -900, Some(-600)),
            ("s3", "t3", -300, None),
        ] {
            sqlx::query(
                "INSERT INTO task_sessions (id, task_id, started_at, ended_at) VALUES (?, ?, ?, ?)"
            )
            .bind(id).bind(task_id)
            .bind((now + chrono::Duration::seconds(start_offset)).to_rfc3339())
            .bind(end_offset.map(|e| (now + chrono::Duration::seconds(e)).to_rfc3339()))
            .execute(&pool).await.unwrap();
        }

        let week_ago = (now - chrono::Duration::days(7)).to_rfc3339();
        let total = get_project_seconds_impl(&pool, "p1", &week_ago).await.unwrap();
        // t1: 300s, t2: 300s = 600
        assert!(total >= 590 && total <= 610, "total={total}");
    }

    #[tokio::test]
    async fn stop_when_no_active_is_noop() {
        let pool = test_pool().await;
        // Не должно падать
        let r = stop_task_tracking_impl(&pool).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn start_nonexistent_task_errors() {
        let pool = test_pool().await;
        let r = start_task_tracking_impl(&pool, "no-such-task").await;
        assert!(r.is_err());
    }
}
