use sqlx::SqlitePool;
use sqlx::migrate::MigrateDatabase;

pub async fn init_db(db_path: &str) -> Result<SqlitePool, sqlx::Error> {
    if !sqlx::Sqlite::database_exists(db_path).await.unwrap_or(false) {
        sqlx::Sqlite::create_database(db_path).await?;
    }

    let pool = SqlitePool::connect(db_path).await?;

    sqlx::migrate!("./src/db/migrations")
        .run(&pool)
        .await
        .map_err(|e: sqlx::migrate::MigrateError| sqlx::Error::Protocol(e.to_string()))?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    // init_db работает с файловой БД (create_database/database_exists), поэтому
    // тестируем на временном файле, а не sqlite::memory:.
    fn temp_db_url() -> (String, std::path::PathBuf) {
        let path = std::env::temp_dir()
            .join(format!("ai-notes-test-{}.db", uuid::Uuid::new_v4()));
        (format!("sqlite:{}?mode=rwc", path.display()), path)
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[tokio::test]
    async fn init_db_creates_file_and_applies_all_migrations() {
        let (url, path) = temp_db_url();
        let pool = init_db(&url).await.expect("init_db failed");

        // Все ключевые таблицы из миграций 0001–0007 на месте
        for table in ["tasks", "notes", "settings", "activity_log", "tasks_fts"] {
            let row = sqlx::query(
                "SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name = ?"
            )
            .bind(table)
            .fetch_optional(&pool)
            .await
            .unwrap();
            assert!(row.is_some(), "таблица {table} не создана миграциями");
        }

        // Повторный init_db на уже существующем файле не падает (идемпотентность)
        drop(pool);
        let pool2 = init_db(&url).await.expect("повторный init_db упал");
        drop(pool2);
        cleanup(&path);
    }

    #[tokio::test]
    async fn fts_triggers_sync_on_insert_update_delete() {
        // Регресс на баг 0004: триггеры tasks_fts должны работать по rowid,
        // иначе после UPDATE индекс расходится и MATCH падает "malformed".
        let (url, path) = temp_db_url();
        let pool = init_db(&url).await.unwrap();

        sqlx::query(
            "INSERT INTO tasks (id, title, created_at, updated_at)
             VALUES (?, ?, ?, ?)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind("покормить кота")
        .bind("2026-07-09T10:00:00+00:00")
        .bind("2026-07-09T10:00:00+00:00")
        .execute(&pool).await.unwrap();

        let found: i64 = sqlx::query("SELECT COUNT(*) AS c FROM tasks_fts WHERE tasks_fts MATCH ?")
            .bind("кот*")
            .fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(found, 1, "FTS не нашёл задачу после INSERT");

        // UPDATE: старый заголовок больше не ищется, новый — ищется, без malformed
        sqlx::query("UPDATE tasks SET title = ? WHERE title = ?")
            .bind("полить цветы").bind("покормить кота")
            .execute(&pool).await.unwrap();

        let old_gone: i64 = sqlx::query("SELECT COUNT(*) AS c FROM tasks_fts WHERE tasks_fts MATCH ?")
            .bind("кот*")
            .fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(old_gone, 0, "FTS всё ещё находит старый заголовок после UPDATE");

        let new_found: i64 = sqlx::query("SELECT COUNT(*) AS c FROM tasks_fts WHERE tasks_fts MATCH ?")
            .bind("цвет*")
            .fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(new_found, 1, "FTS не нашёл задачу по новому заголовку");

        // DELETE: индекс очищается
        sqlx::query("DELETE FROM tasks WHERE title = ?")
            .bind("полить цветы")
            .execute(&pool).await.unwrap();
        let after_delete: i64 = sqlx::query("SELECT COUNT(*) AS c FROM tasks_fts WHERE tasks_fts MATCH ?")
            .bind("цвет*")
            .fetch_one(&pool).await.unwrap().get("c");
        assert_eq!(after_delete, 0, "FTS не очистился после DELETE");

        drop(pool);
        cleanup(&path);
    }
}