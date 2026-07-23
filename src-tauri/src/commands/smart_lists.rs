use tauri::State;
use sqlx::{Row, SqlitePool};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::{AppError, AppResult};

// Предикат умного списка: все заданные поля должны совпасть (AND). Пустой
// объект — бессмысленный список, отклоняется на создании. Хранится как JSON
// в smart_lists.filter_json; встроенные списки («Просроченные», «На этой
// неделе») в БД не заводятся — их логика зависит от текущей даты и целиком
// живёт на фронте.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct SmartListFilter {
    pub category: Option<String>,
    pub priority: Option<String>,
    pub tag: Option<String>,
    pub has_deadline: Option<bool>,
}

impl SmartListFilter {
    fn is_empty(&self) -> bool {
        self.category.is_none() && self.priority.is_none() && self.tag.is_none() && self.has_deadline.is_none()
    }
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SmartList {
    pub id: String,
    pub name: String,
    pub filter: SmartListFilter,
    pub position: i64,
}

fn row_to_smart_list(row: sqlx::sqlite::SqliteRow) -> SmartList {
    let filter_json: String = row.get("filter_json");
    SmartList {
        id: row.get("id"),
        name: row.get("name"),
        filter: serde_json::from_str(&filter_json).unwrap_or_default(),
        position: row.get("position"),
    }
}

#[tauri::command]
pub async fn get_smart_lists(pool: State<'_, SqlitePool>) -> AppResult<Vec<SmartList>> {
    get_smart_lists_impl(pool.inner()).await
}

pub async fn get_smart_lists_impl(pool: &SqlitePool) -> AppResult<Vec<SmartList>> {
    let rows = sqlx::query("SELECT id, name, filter_json, position FROM smart_lists ORDER BY position, name")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_smart_list).collect())
}

#[tauri::command]
pub async fn create_smart_list(pool: State<'_, SqlitePool>, name: String, filter: SmartListFilter) -> AppResult<SmartList> {
    create_smart_list_impl(pool.inner(), name, filter).await
}

pub async fn create_smart_list_impl(pool: &SqlitePool, name: String, filter: SmartListFilter) -> AppResult<SmartList> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Other("Название списка не может быть пустым".into()));
    }
    if filter.is_empty() {
        return Err(AppError::Other("Список без условий фильтра не имеет смысла".into()));
    }
    let position: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM smart_lists")
        .fetch_one(pool)
        .await?;
    let id = Uuid::new_v4().to_string();
    let filter_json = serde_json::to_string(&filter).unwrap_or_else(|_| "{}".into());
    sqlx::query("INSERT INTO smart_lists (id, name, filter_json, position) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&filter_json)
        .bind(position)
        .execute(pool)
        .await?;
    Ok(SmartList { id, name, filter, position })
}

#[tauri::command]
pub async fn delete_smart_list(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_smart_list_impl(pool.inner(), &id).await
}

pub async fn delete_smart_list_impl(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM smart_lists WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn filter_cat(cat: &str) -> SmartListFilter {
        SmartListFilter { category: Some(cat.into()), ..Default::default() }
    }

    #[tokio::test]
    async fn create_get_delete_roundtrip() {
        let pool = test_pool().await;
        let l = create_smart_list_impl(&pool, "Работа".into(), filter_cat("Work")).await.unwrap();
        assert_eq!(l.name, "Работа");
        assert_eq!(l.filter.category, Some("Work".into()));

        let all = get_smart_lists_impl(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].filter, filter_cat("Work"));

        delete_smart_list_impl(&pool, &l.id).await.unwrap();
        assert!(get_smart_lists_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_name_rejected() {
        let pool = test_pool().await;
        let r = create_smart_list_impl(&pool, "   ".into(), filter_cat("Work")).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn empty_filter_rejected() {
        let pool = test_pool().await;
        let r = create_smart_list_impl(&pool, "Пустой".into(), SmartListFilter::default()).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn filter_json_roundtrips_all_fields() {
        let pool = test_pool().await;
        let filter = SmartListFilter {
            category: Some("Work".into()),
            priority: Some("High".into()),
            tag: Some("важное".into()),
            has_deadline: Some(true),
        };
        let l = create_smart_list_impl(&pool, "Комплексный".into(), filter.clone()).await.unwrap();
        let all = get_smart_lists_impl(&pool).await.unwrap();
        assert_eq!(all.iter().find(|x| x.id == l.id).unwrap().filter, filter);
    }

    #[tokio::test]
    async fn list_ordered_by_position_then_name() {
        let pool = test_pool().await;
        create_smart_list_impl(&pool, "Бета".into(), filter_cat("Work")).await.unwrap();
        create_smart_list_impl(&pool, "Альфа".into(), filter_cat("Study")).await.unwrap();
        let all = get_smart_lists_impl(&pool).await.unwrap();
        assert_eq!(all.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["Бета", "Альфа"]);
    }
}
