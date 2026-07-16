use tauri::State;
use sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::AppResult;

// Системный фолбэк: не удаляется, принимает задачи удалённых категорий
// и невалидные значения category при записи задач.
pub const FALLBACK_CATEGORY: &str = "Other";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, sqlx::FromRow)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub color: String,
    pub position: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateCategory {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[tauri::command]
pub async fn get_categories(pool: State<'_, SqlitePool>) -> AppResult<Vec<Category>> {
    get_categories_impl(pool.inner()).await
}

pub async fn get_categories_impl(pool: &SqlitePool) -> AppResult<Vec<Category>> {
    Ok(sqlx::query_as::<_, Category>(
        "SELECT id, name, color, position FROM categories ORDER BY position, name",
    )
    .fetch_all(pool)
    .await?)
}

#[tauri::command]
pub async fn create_category(pool: State<'_, SqlitePool>, name: String, color: String) -> AppResult<Category> {
    create_category_impl(pool.inner(), name, color).await
}

pub async fn create_category_impl(pool: &SqlitePool, name: String, color: String) -> AppResult<Category> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Название категории не может быть пустым".to_string().into());
    }
    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM categories")
        .fetch_one(pool)
        .await?;
    let cat = Category {
        id: Uuid::new_v4().to_string(),
        name,
        color: if color.trim().is_empty() { "#888888".into() } else { color },
        position,
    };
    sqlx::query("INSERT INTO categories (id, name, color, position) VALUES (?, ?, ?, ?)")
        .bind(&cat.id)
        .bind(&cat.name)
        .bind(&cat.color)
        .bind(cat.position)
        .execute(pool)
        .await?;
    Ok(cat)
}

#[tauri::command]
pub async fn update_category(pool: State<'_, SqlitePool>, id: String, patch: UpdateCategory) -> AppResult<()> {
    update_category_impl(pool.inner(), id, patch).await
}

pub async fn update_category_impl(pool: &SqlitePool, id: String, patch: UpdateCategory) -> AppResult<()> {
    if let Some(name) = patch.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Название категории не может быть пустым".to_string().into());
        }
        sqlx::query("UPDATE categories SET name = ? WHERE id = ?")
            .bind(&name).bind(&id)
            .execute(pool).await?;
    }
    if let Some(color) = patch.color {
        sqlx::query("UPDATE categories SET color = ? WHERE id = ?")
            .bind(&color).bind(&id)
            .execute(pool).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_category(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_category_impl(pool.inner(), id).await
}

pub async fn delete_category_impl(pool: &SqlitePool, id: String) -> AppResult<()> {
    if id == FALLBACK_CATEGORY {
        return Err("Категорию «Другое» нельзя удалить — это фолбэк".to_string().into());
    }
    // Задачи удаляемой категории переезжают в фолбэк
    sqlx::query("UPDATE tasks SET category = ? WHERE category = ?")
        .bind(FALLBACK_CATEGORY)
        .bind(&id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM categories WHERE id = ?")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(())
}

// Валидация категории на записи задачи: неизвестный id тихо становится
// фолбэком (прежняя семантика enum: неизвестное → Other).
pub async fn valid_or_fallback(pool: &SqlitePool, category: &str) -> String {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM categories WHERE id = ?")
        .bind(category)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    if exists.is_some() {
        category.to_string()
    } else {
        FALLBACK_CATEGORY.to_string()
    }
}

// Сопоставление ответа модели с категорией: по имени или id, без учёта
// регистра и обрамляющей пунктуации. Чистая функция для ai_classify.
pub fn match_category(categories: &[Category], answer: &str) -> Option<String> {
    let norm = answer
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if norm.is_empty() {
        return None;
    }
    categories
        .iter()
        .find(|c| c.name.to_lowercase() == norm || c.id.to_lowercase() == norm)
        .map(|c| c.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn seeded_categories_present_and_ordered() {
        let pool = test_pool().await;
        let cats = get_categories_impl(&pool).await.unwrap();
        let ids: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["Work", "Study", "Home", "Health", "Other"]);
        assert_eq!(cats[0].name, "Работа");
        assert!(cats.iter().all(|c| !c.color.is_empty()));
    }

    #[tokio::test]
    async fn crud_roundtrip_and_position() {
        let pool = test_pool().await;

        let cat = create_category_impl(&pool, "Спорт".into(), "#ff0000".into()).await.unwrap();
        assert_eq!(cat.position, 5); // после посевных 0..4

        update_category_impl(&pool, cat.id.clone(), UpdateCategory {
            name: Some("Тренировки".into()),
            color: Some("#00ff00".into()),
        }).await.unwrap();
        let cats = get_categories_impl(&pool).await.unwrap();
        let found = cats.iter().find(|c| c.id == cat.id).unwrap();
        assert_eq!(found.name, "Тренировки");
        assert_eq!(found.color, "#00ff00");

        assert!(create_category_impl(&pool, "   ".into(), "".into()).await.is_err());

        delete_category_impl(&pool, cat.id.clone()).await.unwrap();
        assert!(get_categories_impl(&pool).await.unwrap().iter().all(|c| c.id != cat.id));
    }

    #[tokio::test]
    async fn delete_reassigns_tasks_and_protects_fallback() {
        let pool = test_pool().await;
        let cat = create_category_impl(&pool, "Временная".into(), "#123456".into()).await.unwrap();

        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden, created_at, updated_at)
             VALUES ('t1', 'задача', 'Todo', 'Medium', ?, 'None', '[]', 0, '2026-07-16T10:00:00+00:00', '2026-07-16T10:00:00+00:00')",
        )
        .bind(&cat.id)
        .execute(&pool).await.unwrap();

        delete_category_impl(&pool, cat.id).await.unwrap();
        let task_cat: String = sqlx::query_scalar("SELECT category FROM tasks WHERE id = 't1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(task_cat, FALLBACK_CATEGORY);

        assert!(delete_category_impl(&pool, FALLBACK_CATEGORY.into()).await.is_err());
    }

    #[tokio::test]
    async fn valid_or_fallback_checks_table() {
        let pool = test_pool().await;
        assert_eq!(valid_or_fallback(&pool, "Work").await, "Work");
        assert_eq!(valid_or_fallback(&pool, "???").await, "Other");
        let cat = create_category_impl(&pool, "Новая".into(), "".into()).await.unwrap();
        assert_eq!(valid_or_fallback(&pool, &cat.id).await, cat.id);
    }

    #[test]
    fn match_category_by_name_id_case_and_punctuation() {
        let cats = vec![
            Category { id: "Work".into(), name: "Работа".into(), color: "".into(), position: 0 },
            Category { id: "abc-123".into(), name: "Спорт".into(), color: "".into(), position: 1 },
        ];
        assert_eq!(match_category(&cats, "Работа"), Some("Work".into()));
        assert_eq!(match_category(&cats, "  работа.  "), Some("Work".into()));
        assert_eq!(match_category(&cats, "WORK"), Some("Work".into()));
        assert_eq!(match_category(&cats, "«Спорт»"), Some("abc-123".into()));
        assert_eq!(match_category(&cats, "Отдых"), None);
        assert_eq!(match_category(&cats, ""), None);
    }
}
