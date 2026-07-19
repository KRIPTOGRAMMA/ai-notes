use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use tauri::Manager;
use crate::error::AppResult;
use crate::commands::settings::{get_setting, set_setting};

// БД работает в WAL-режиме: свежие записи лежат в data.db-wal, а не в data.db.
// Просто скопировать файл нельзя — снимок будет неполным. VACUUM INTO пишет
// целостную копию всей БД (включая WAL) в отдельный файл.
pub async fn export_impl(pool: &sqlx::SqlitePool, data_dir: &Path, path: &str) -> AppResult<()> {
    let snapshot_path = data_dir.join("data.db.export");

    let _ = std::fs::remove_file(&snapshot_path); // VACUUM INTO требует, чтобы файла не было
    sqlx::query("VACUUM INTO ?")
        .bind(snapshot_path.to_string_lossy().as_ref())
        .execute(pool)
        .await?;

    let result: AppResult<()> = (|| {
        let zip_file = File::create(path)?;
        let mut zip = ZipWriter::new(zip_file);
        let options = SimpleFileOptions::default();

        zip.start_file("data.db", options)?;
        let mut db_file = File::open(&snapshot_path)?;
        let mut buf = Vec::new();
        db_file.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;

        zip.finish()?;
        Ok(())
    })();

    let _ = std::fs::remove_file(&snapshot_path);
    result
}

#[tauri::command]
pub async fn export(
    app: tauri::AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
    path: String,
) -> AppResult<()> {
    let data_dir = app.path().app_data_dir()?;
    export_impl(pool.inner(), &data_dir, &path).await
}

// Нельзя перезаписывать data.db на живом пуле: activity-loop пишет в БД
// каждые 60 сек и затёр бы импорт. Кладём staging-файл и перезапускаем
// приложение — apply_pending_import() подхватит его до открытия пула.
pub fn import_impl(data_dir: &Path, path: &str) -> AppResult<()> {
    let staging_path = data_dir.join("data.db.import");

    let zip_file = File::open(path)?;
    let mut archive = ZipArchive::new(zip_file)?;

    let mut entry = archive.by_name("data.db")?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;

    std::fs::write(&staging_path, &buf)?;
    Ok(())
}

#[tauri::command]
pub async fn import(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let data_dir = app.path().app_data_dir()?;
    import_impl(&data_dir, &path)?;
    app.restart()
}

// Определяет, нужно ли делать авто-бэкап: прошло ≥24ч с последнего
// и папка задана.
pub async fn auto_backup_due(pool: &sqlx::SqlitePool) -> bool {
    let dir = get_setting(pool, "auto_backup_dir").await;
    let dir = match dir {
        Some(d) if !d.trim().is_empty() => d,
        _ => return false,
    };
    if !Path::new(&dir).is_dir() {
        return false;
    }
    let last = get_setting(pool, "last_auto_backup").await;
    match last {
        Some(ts) => {
            let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&ts) else {
                return true;
            };
            let elapsed = chrono::Utc::now() - parsed.with_timezone(&chrono::Utc);
            elapsed >= chrono::Duration::hours(24)
        }
        None => true, // ни разу не делали — пора
    }
}

// Выполняет авто-бэкап: экспорт + ротация. Возвращает имя файла.
pub async fn auto_backup_impl(
    pool: &sqlx::SqlitePool,
    data_dir: &Path,
) -> AppResult<String> {
    let backup_dir = get_setting(pool, "auto_backup_dir").await.unwrap_or_default();
    let keep: usize = get_setting(pool, "auto_backup_keep").await
        .and_then(|v| v.parse().ok())
        .unwrap_or(7)
        .max(1);

    let dir = PathBuf::from(&backup_dir);
    let now = chrono::Local::now();
    let filename = format!("ai-notes-backup-{}.zip", now.format("%Y-%m-%d-%H%M"));
    let path = dir.join(&filename);

    export_impl(pool, data_dir, path.to_str().unwrap()).await?;

    // Ротация: удаляем старейшие файлы сверх keep
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("cannot read backup dir {:?}", dir))
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("ai-notes-backup-")
                && e.file_name().to_string_lossy().ends_with(".zip")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    while entries.len() > keep {
        if let Some(oldest) = entries.first() {
            let _ = fs::remove_file(oldest.path());
            entries.remove(0);
        }
    }

    set_setting(pool, "last_auto_backup", &now.to_rfc3339()).await?;
    Ok(filename)
}

#[tauri::command]
pub async fn do_auto_backup(
    app: tauri::AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    auto_backup_impl(pool.inner(), &data_dir)
        .await
        .map_err(|e| e.to_string())
}

pub fn apply_pending_import(data_dir: &std::path::Path) {
    let staging = data_dir.join("data.db.import");
    if staging.exists() {
        let _ = std::fs::rename(&staging, data_dir.join("data.db"));
        // Остатки WAL от старой БД иначе "переиграются" поверх импортированной
        // и молча откатят импорт.
        let _ = std::fs::remove_file(data_dir.join("data.db-wal"));
        let _ = std::fs::remove_file(data_dir.join("data.db-shm"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ai-notes-test-{}-{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // Не sqlite::memory: — у пула к in-memory БД каждое соединение видит свою
    // пустую базу, и VACUUM INTO может уйти не туда. Файловая БД повторяет прод.
    async fn test_pool(dir: &Path) -> sqlx::SqlitePool {
        let url = format!("sqlite:{}?mode=rwc", dir.join("source.db").display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_task(pool: &sqlx::SqlitePool, title: &str) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden, created_at, updated_at)
             VALUES (?, ?, 'Todo', 'Medium', 'Work', 'None', '[]', 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(title)
            .execute(pool).await.unwrap();
    }

    // Полный цикл: экспорт в zip → импорт в staging → применение при «рестарте» →
    // открытие импортированной БД и проверка данных.
    #[tokio::test]
    async fn export_import_round_trip() {
        let dir = tmp_dir("roundtrip");
        let pool = test_pool(&dir).await;
        insert_task(&pool, "задача для бэкапа").await;

        let zip_path = dir.join("backup.zip");
        export_impl(&pool, &dir, zip_path.to_str().unwrap()).await.unwrap();
        assert!(zip_path.exists());
        // временный снимок VACUUM INTO подчищен
        assert!(!dir.join("data.db.export").exists());

        import_impl(&dir, zip_path.to_str().unwrap()).unwrap();
        assert!(dir.join("data.db.import").exists());

        // Симулируем состояние до рестарта: старая БД с WAL-остатками
        std::fs::write(dir.join("data.db"), b"old-db").unwrap();
        std::fs::write(dir.join("data.db-wal"), b"stale-wal").unwrap();
        std::fs::write(dir.join("data.db-shm"), b"stale-shm").unwrap();
        apply_pending_import(&dir);
        assert!(!dir.join("data.db.import").exists());
        assert!(!dir.join("data.db-wal").exists());
        assert!(!dir.join("data.db-shm").exists());

        let imported = sqlx::SqlitePool::connect(&format!("sqlite:{}", dir.join("data.db").display()))
            .await
            .unwrap();
        let title: String = sqlx::query_scalar("SELECT title FROM tasks")
            .fetch_one(&imported)
            .await
            .unwrap();
        assert_eq!(title, "задача для бэкапа");

        imported.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn import_rejects_non_zip() {
        let dir = tmp_dir("badzip");
        let bad = dir.join("not-a-zip.zip");
        std::fs::write(&bad, b"garbage").unwrap();

        assert!(import_impl(&dir, bad.to_str().unwrap()).is_err());
        // staging-файл не должен появиться
        assert!(!dir.join("data.db.import").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_pending_import_is_noop_without_staging() {
        let dir = tmp_dir("noop");
        std::fs::write(dir.join("data.db"), b"current").unwrap();
        apply_pending_import(&dir);
        assert_eq!(std::fs::read(dir.join("data.db")).unwrap(), b"current");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auto_backup_due_returns_false_when_dir_empty() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        set_setting(&pool, "auto_backup_dir", "").await.unwrap();
        assert!(!auto_backup_due(&pool).await);
    }

    #[tokio::test]
    async fn auto_backup_due_returns_true_when_no_last_backup() {
        let dir = tmp_dir("due_no_last");
        let backup_dir = dir.join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        set_setting(&pool, "auto_backup_dir", backup_dir.to_str().unwrap()).await.unwrap();
        assert!(auto_backup_due(&pool).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auto_backup_due_respects_24h_interval() {
        let dir = tmp_dir("due_24h");
        let backup_dir = dir.join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        set_setting(&pool, "auto_backup_dir", backup_dir.to_str().unwrap()).await.unwrap();

        // недавний бэкап — не должен срабатывать
        let recent = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        set_setting(&pool, "last_auto_backup", &recent).await.unwrap();
        assert!(!auto_backup_due(&pool).await);

        // 25 часов назад — должен
        let old = (Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        set_setting(&pool, "last_auto_backup", &old).await.unwrap();
        assert!(auto_backup_due(&pool).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn auto_backup_rotation_keeps_only_keep_files() {
        let dir = tmp_dir("rotation");
        let backup_dir = dir.join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        // Чужие файлы не трогаем
        std::fs::write(backup_dir.join("note.txt"), b"hello").unwrap();

        // Создаём «старые» бэкапы — через прямой export_impl, не auto_backup_impl
        let pool = test_pool(&dir).await;
        set_setting(&pool, "auto_backup_dir", backup_dir.to_str().unwrap()).await.unwrap();
        set_setting(&pool, "auto_backup_keep", "3").await.unwrap();

        // Симулируем 4 старых бэкапа
        for i in 1..=4 {
            let name = format!("ai-notes-backup-2026-07-{:02}0-1200.zip", i);
            std::fs::write(backup_dir.join(&name), b"fake-zip").unwrap();
        }

        // Запускаем авто-бэкап — он создаст новый и почистит старые
        auto_backup_impl(&pool, &dir).await.unwrap();

        // Должно остаться 3 бэкапа (keep) + 1 чужой файл = 4 файла
        let mut entries: Vec<_> = std::fs::read_dir(&backup_dir).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let backup_count = entries.iter().filter(|e| {
            e.file_name().to_string_lossy().starts_with("ai-notes-backup-")
        }).count();
        assert_eq!(backup_count, 3, "должно быть 3 бэкапа после ротации");

        // Чужой файл не тронут
        assert!(backup_dir.join("note.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
