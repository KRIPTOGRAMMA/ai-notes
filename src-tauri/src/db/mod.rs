use sqlx::SqlitePool;

pub async fn init_db(db_path: &str) -> Result<SqlitePool, sqlx::Error> {
  let pool = SqlitePool::connect(db_path).await?;

  sqlx::query("
    CREATE TABLE IF NOT EXISTS tasks (
      id TEXT,
      title TEXT,
      description TEXT,
      status TEXT,
      priority TEXT,
      category TEXT,
      deadline DATETIME,
      tags TEXT,
      created_at DATETIME,
      updated_at DATETIME
    )
  ")
    .execute(&pool)
    .await?;
  
  Ok(pool)
}