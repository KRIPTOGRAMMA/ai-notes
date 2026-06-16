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