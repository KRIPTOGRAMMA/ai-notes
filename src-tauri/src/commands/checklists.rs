use tauri::State;
use sqlx::{Row, SqlitePool};
use serde::Serialize;
use uuid::Uuid;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ChecklistTemplate {
    pub id: String,
    pub name: String,
    pub items: Vec<String>,
}

fn row_to_template(row: sqlx::sqlite::SqliteRow) -> ChecklistTemplate {
    let items_json: String = row.get("items");
    ChecklistTemplate {
        id: row.get("id"),
        name: row.get("name"),
        items: serde_json::from_str(&items_json).unwrap_or_default(),
    }
}

#[tauri::command]
pub async fn get_checklist_templates(pool: State<'_, SqlitePool>) -> AppResult<Vec<ChecklistTemplate>> {
    get_checklist_templates_impl(pool.inner()).await
}

pub async fn get_checklist_templates_impl(pool: &SqlitePool) -> AppResult<Vec<ChecklistTemplate>> {
    let rows = sqlx::query("SELECT id, name, items FROM checklist_templates ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(row_to_template).collect())
}

#[tauri::command]
pub async fn create_checklist_template(
    pool: State<'_, SqlitePool>,
    name: String,
    items: Vec<String>,
) -> AppResult<ChecklistTemplate> {
    create_checklist_template_impl(pool.inner(), name, items).await
}

pub async fn create_checklist_template_impl(
    pool: &SqlitePool,
    name: String,
    items: Vec<String>,
) -> AppResult<ChecklistTemplate> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Other("Название шаблона не может быть пустым".into()));
    }
    let items: Vec<String> = items.into_iter().map(|i| i.trim().to_string()).filter(|i| !i.is_empty()).collect();
    if items.is_empty() {
        return Err(AppError::Other("Шаблон без пунктов не имеет смысла".into()));
    }

    let id = Uuid::new_v4().to_string();
    let items_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".into());
    sqlx::query("INSERT INTO checklist_templates (id, name, items) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&items_json)
        .execute(pool)
        .await?;

    Ok(ChecklistTemplate { id, name, items })
}

#[tauri::command]
pub async fn delete_checklist_template(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_checklist_template_impl(pool.inner(), &id).await
}

pub async fn delete_checklist_template_impl(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM checklist_templates WHERE id = ?")
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

    #[tokio::test]
    async fn create_get_delete_roundtrip() {
        let pool = test_pool().await;
        let t = create_checklist_template_impl(&pool, "Поездка".into(), vec!["паспорт".into(), "билеты".into()]).await.unwrap();
        assert_eq!(t.name, "Поездка");
        assert_eq!(t.items, vec!["паспорт", "билеты"]);

        let all = get_checklist_templates_impl(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].items, vec!["паспорт", "билеты"]);

        delete_checklist_template_impl(&pool, &t.id).await.unwrap();
        assert!(get_checklist_templates_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn items_json_roundtrip_preserves_order_and_trims() {
        let pool = test_pool().await;
        let t = create_checklist_template_impl(&pool, "  С пробелами  ".into(), vec![" один ".into(), "два".into(), "  три".into()]).await.unwrap();
        assert_eq!(t.name, "С пробелами");
        assert_eq!(t.items, vec!["один", "два", "три"]);

        let all = get_checklist_templates_impl(&pool).await.unwrap();
        assert_eq!(all[0].items, vec!["один", "два", "три"]);
    }

    #[tokio::test]
    async fn empty_items_are_filtered_out() {
        let pool = test_pool().await;
        let t = create_checklist_template_impl(&pool, "x".into(), vec!["a".into(), "  ".into(), "b".into()]).await.unwrap();
        assert_eq!(t.items, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn empty_name_rejected() {
        let pool = test_pool().await;
        let r = create_checklist_template_impl(&pool, "   ".into(), vec!["a".into()]).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn all_empty_items_rejected() {
        let pool = test_pool().await;
        let r = create_checklist_template_impl(&pool, "x".into(), vec!["  ".into(), "".into()]).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn list_ordered_by_name() {
        let pool = test_pool().await;
        create_checklist_template_impl(&pool, "Бета".into(), vec!["a".into()]).await.unwrap();
        create_checklist_template_impl(&pool, "Альфа".into(), vec!["a".into()]).await.unwrap();
        let all = get_checklist_templates_impl(&pool).await.unwrap();
        assert_eq!(all.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["Альфа", "Бета"]);
    }
}
