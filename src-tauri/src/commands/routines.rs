use chrono::{Datelike, Local, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::State;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Routine {
    pub id: String,
    pub title: String,
    pub days_mask: i64,
    pub start_mins: i64,
    pub duration_mins: i64,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoutine {
    pub title: String,
    pub days_mask: i64,
    pub start_mins: i64,
    pub duration_mins: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateRoutine {
    pub title: Option<String>,
    pub days_mask: Option<i64>,
    pub start_mins: Option<i64>,
    pub duration_mins: Option<i64>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutineBlock {
    pub title: String,
    pub start_mins: i64,     // минут от полуночи
    pub duration_mins: i64,
}

#[tauri::command]
pub async fn get_routines(pool: State<'_, SqlitePool>) -> AppResult<Vec<Routine>> {
    let rows = sqlx::query("SELECT id, title, days_mask, start_mins, duration_mins, active FROM routines ORDER BY start_mins")
        .fetch_all(pool.inner())
        .await?;
    Ok(rows.iter().map(row_to_routine).collect())
}

#[tauri::command]
pub async fn create_routine(
    pool: State<'_, SqlitePool>,
    routine: CreateRoutine,
) -> AppResult<Routine> {
    create_routine_impl(pool.inner(), routine).await
}

pub async fn create_routine_impl(pool: &SqlitePool, routine: CreateRoutine) -> AppResult<Routine> {
    if routine.title.trim().is_empty() {
        return Err("Название рутины не может быть пустым".to_string().into());
    }
    let r = Routine {
        id: uuid::Uuid::new_v4().to_string(),
        title: routine.title.trim().to_string(),
        days_mask: routine.days_mask,
        start_mins: routine.start_mins,
        duration_mins: routine.duration_mins.max(15),
        active: true,
    };
    sqlx::query(
        "INSERT INTO routines (id, title, days_mask, start_mins, duration_mins, active) VALUES (?, ?, ?, ?, ?, 1)"
    )
    .bind(&r.id).bind(&r.title).bind(r.days_mask).bind(r.start_mins).bind(r.duration_mins)
    .execute(pool).await?;
    Ok(r)
}

#[tauri::command]
pub async fn update_routine(
    pool: State<'_, SqlitePool>,
    id: String,
    patch: UpdateRoutine,
) -> AppResult<()> {
    update_routine_impl(pool.inner(), id, patch).await
}

pub async fn update_routine_impl(pool: &SqlitePool, id: String, patch: UpdateRoutine) -> AppResult<()> {
    if let Some(title) = &patch.title {
        if title.trim().is_empty() {
            return Err("Название рутины не может быть пустым".to_string().into());
        }
        sqlx::query("UPDATE routines SET title = ? WHERE id = ?")
            .bind(title.trim()).bind(&id).execute(pool).await?;
    }
    if let Some(mask) = patch.days_mask {
        sqlx::query("UPDATE routines SET days_mask = ? WHERE id = ?")
            .bind(mask).bind(&id).execute(pool).await?;
    }
    if let Some(mins) = patch.start_mins {
        sqlx::query("UPDATE routines SET start_mins = ? WHERE id = ?")
            .bind(mins).bind(&id).execute(pool).await?;
    }
    if let Some(mins) = patch.duration_mins {
        sqlx::query("UPDATE routines SET duration_mins = ? WHERE id = ?")
            .bind(mins.max(15)).bind(&id).execute(pool).await?;
    }
    if let Some(active) = patch.active {
        sqlx::query("UPDATE routines SET active = ? WHERE id = ?")
            .bind(active).bind(&id).execute(pool).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_routine(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    sqlx::query("DELETE FROM routines WHERE id = ?").bind(&id).execute(pool.inner()).await?;
    Ok(())
}

/// Возвращает все активные рутины для указанного дня недели (0=пн, 6=вс).
pub async fn routines_for_day(pool: &SqlitePool, weekday: u32) -> Result<Vec<RoutineBlock>, sqlx::Error> {
    let bit: i64 = 1 << weekday;
    let rows = sqlx::query(
        "SELECT title, start_mins, duration_mins FROM routines
         WHERE active = 1 AND (days_mask & ?) != 0
         ORDER BY start_mins"
    )
    .bind(bit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| RoutineBlock {
        title: r.get("title"),
        start_mins: r.get("start_mins"),
        duration_mins: r.get("duration_mins"),
    }).collect())
}

/// Возвращает сегодняшние рутины как busy-слоты (start_mins, end_mins, title).
pub async fn today_routine_busy(pool: &SqlitePool) -> Result<Vec<(i64, i64, String)>, sqlx::Error> {
    let weekday = Utc::now().with_timezone(&Local).date_naive().weekday().num_days_from_monday();
    let blocks = routines_for_day(pool, weekday).await?;
    Ok(blocks.into_iter().map(|b| (b.start_mins, b.start_mins + b.duration_mins, b.title)).collect())
}

fn row_to_routine(r: &sqlx::sqlite::SqliteRow) -> Routine {
    Routine {
        id: r.get("id"),
        title: r.get("title"),
        days_mask: r.get("days_mask"),
        start_mins: r.get("start_mins"),
        duration_mins: r.get("duration_mins"),
        active: r.get::<i64, _>("active") != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn mask(days: &[u32]) -> i64 {
        days.iter().fold(0i64, |acc, d| acc | (1 << d))
    }

    #[tokio::test]
    async fn create_and_get() {
        let pool = test_pool().await;
        let r = create_routine_impl(&pool, CreateRoutine {
            title: "Зарядка".into(),
            days_mask: mask(&[0, 2, 4]),
            start_mins: 7 * 60,
            duration_mins: 30,
        }).await.unwrap();
        assert_eq!(r.title, "Зарядка");
        assert!(r.active);

        let list = sqlx::query_as::<_, (String, String, i64, i64, i64, i64)>(
            "SELECT id, title, days_mask, start_mins, duration_mins, active FROM routines"
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].1, "Зарядка");
    }

    #[tokio::test]
    async fn empty_title_rejected() {
        let pool = test_pool().await;
        let r = create_routine_impl(&pool, CreateRoutine {
            title: "  ".into(),
            days_mask: 127,
            start_mins: 0,
            duration_mins: 30,
        }).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn update_routine_fields() {
        let pool = test_pool().await;
        let r = create_routine_impl(&pool, CreateRoutine {
            title: "Старая рутина".into(),
            days_mask: 1,
            start_mins: 8 * 60,
            duration_mins: 30,
        }).await.unwrap();

        update_routine_impl(&pool, r.id.clone(), UpdateRoutine {
            title: Some("Новая рутина".into()),
            days_mask: Some(3),
            start_mins: Some(9 * 60),
            duration_mins: Some(45),
            active: Some(false),
        }).await.unwrap();

        let rows = sqlx::query("SELECT title, days_mask, start_mins, duration_mins, active FROM routines WHERE id = ?")
            .bind(&r.id).fetch_one(&pool).await.unwrap();
        assert_eq!(rows.get::<String, _>("title"), "Новая рутина");
        assert_eq!(rows.get::<i64, _>("days_mask"), 3);
        assert_eq!(rows.get::<i64, _>("start_mins"), 9 * 60);
        assert_eq!(rows.get::<i64, _>("duration_mins"), 45);
        assert_eq!(rows.get::<i64, _>("active"), 0);
    }

    #[tokio::test]
    async fn days_mask_bits() {
        let pool = test_pool().await;
        // пн (0) и ср (2)
        create_routine_impl(&pool, CreateRoutine {
            title: "Зарядка".into(),
            days_mask: mask(&[0, 2]),
            start_mins: 7 * 60,
            duration_mins: 30,
        }).await.unwrap();

        let mon = routines_for_day(&pool, 0).await.unwrap();
        assert_eq!(mon.len(), 1);

        let tue = routines_for_day(&pool, 1).await.unwrap();
        assert_eq!(tue.len(), 0);

        let wed = routines_for_day(&pool, 2).await.unwrap();
        assert_eq!(wed.len(), 1);
    }

    #[tokio::test]
    async fn inactive_not_returned() {
        let pool = test_pool().await;
        let r = create_routine_impl(&pool, CreateRoutine {
            title: "Off".into(),
            days_mask: 1,
            start_mins: 0,
            duration_mins: 30,
        }).await.unwrap();
        update_routine_impl(&pool, r.id, UpdateRoutine {
            active: Some(false), ..Default::default()
        }).await.unwrap();

        let blocks = routines_for_day(&pool, 0).await.unwrap();
        assert!(blocks.is_empty());
    }

    #[tokio::test]
    async fn delete_removes_routine() {
        let pool = test_pool().await;
        let r = create_routine_impl(&pool, CreateRoutine {
            title: "Удаляемая".into(),
            days_mask: 1,
            start_mins: 0,
            duration_mins: 15,
        }).await.unwrap();

        sqlx::query("DELETE FROM routines WHERE id = ?").bind(&r.id).execute(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM routines")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn routine_busy_overlap_with_planner() {
        let pool = test_pool().await;
        // Рутина с пн по пт (0-4) с 9:00 до 10:00
        create_routine_impl(&pool, CreateRoutine {
            title: "Планёрка".into(),
            days_mask: mask(&[0, 1, 2, 3, 4]),
            start_mins: 9 * 60,
            duration_mins: 60,
        }).await.unwrap();

        let busy = today_routine_busy(&pool).await.unwrap();
        // weekday может быть любым — проверяем структуру
        for (start, end, title) in &busy {
            assert_eq!(*end - *start, 60);
            assert_eq!(title, "Планёрка");
        }
        // Если сегодня пн-пт, должен быть 1 блок
        let wd = Utc::now().with_timezone(&Local).date_naive().weekday().num_days_from_monday();
        if wd < 5 {
            assert_eq!(busy.len(), 1);
        }
    }
}
