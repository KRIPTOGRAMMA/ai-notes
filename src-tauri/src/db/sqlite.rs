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

    // init_db works with a file-backed DB (create_database/database_exists), so we
    // test against a temporary file rather than sqlite::memory:.
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

        // Every key table from migrations 0001-0007 is present
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

        // A second init_db over an existing file does not fail (idempotence)
        drop(pool);
        let pool2 = init_db(&url).await.expect("повторный init_db упал");
        drop(pool2);
        cleanup(&path);
    }

    #[tokio::test]
    async fn fts_triggers_sync_on_insert_update_delete() {
        // Regression for the 0004 bug: the tasks_fts triggers must work by rowid,
        // otherwise the index diverges after an UPDATE and MATCH fails as
        // "malformed".
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

        // UPDATE: the old title no longer matches, the new one does, with no malformed error
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

        // DELETE: the index is cleared
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

    // Three migrations (0017, 0031 x2) rely on ON DELETE CASCADE, and that
    // cascade only runs when SQLite's foreign key enforcement is on. Nothing in
    // this file turns it on: it is on because sqlx sets `PRAGMA foreign_keys` in
    // its default connect options, while raw SQLite defaults it to OFF.
    //
    // So an inherited library default is load-bearing for data integrity. A sqlx
    // upgrade that changed it would not break the build and would not fail any
    // other test — it would silently start leaving orphaned rows behind on every
    // delete. This asserts the assumption directly, then proves the consequence
    // that actually matters.
    //
    // The version that added this test was originally written to fix "orphaned
    // dependencies on purge", on the strength of a sqlite3-CLI experiment showing
    // the pragma off. The CLI has its own default; it was never measuring this
    // connection. Hence: assert the pragma where the app opens its pool.
    #[tokio::test]
    async fn foreign_keys_are_enforced_so_cascades_actually_fire() {
        let (url, path) = temp_db_url();
        let pool = init_db(&url).await.unwrap();

        let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(
            fk, 1,
            "PRAGMA foreign_keys выключен — ON DELETE CASCADE в 0017 и 0031 \
             перестанет срабатывать, и удаление начнёт молча оставлять сироты"
        );

        let blocked = uuid::Uuid::new_v4().to_string();
        let blocker = uuid::Uuid::new_v4().to_string();
        for id in [&blocked, &blocker] {
            sqlx::query("INSERT INTO tasks (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind("живая")
                .bind("2026-08-05T10:00:00+00:00")
                .bind("2026-08-05T10:00:00+00:00")
                .execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO task_dependencies (task_id, blocker_id, created_at) VALUES (?, ?, ?)"
        )
        .bind(&blocked).bind(&blocker).bind("2026-08-05T10:00:00+00:00")
        .execute(&pool).await.unwrap();

        // A bare DELETE, without the manual cleanup purge_deleted_task_impl does
        // — this measures the schema, not the command.
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(&blocker).execute(&pool).await.unwrap();

        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_dependencies")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(left, 0, "каскад из 0031 не сработал: связь пережила удаление блокера");

        drop(pool);
        cleanup(&path);
    }
}
