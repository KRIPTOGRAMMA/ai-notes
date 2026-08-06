use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use tauri::Manager;
use crate::error::AppResult;
use crate::commands::settings::{get_setting, set_setting};

// Turns automatic backups on the first time the app runs, pointing them at a
// folder inside the app's own data dir.
//
// Until v0.9.85 auto_backup_dir defaulted to empty, auto_backup_due returned
// false for an empty dir, and nothing in the UI said so — the result was an app
// that had never once backed itself up while looking like it might have. Off by
// default is a defensible choice only when the user is told; it wasn't.
//
// The marker key is what separates "never configured" from "deliberately turned
// off". Without it, clearing the folder in Settings would be undone on the next
// launch and the setting would be impossible to switch off.
pub async fn ensure_default_backup_dir(pool: &sqlx::SqlitePool, data_dir: &Path) -> AppResult<()> {
    if get_setting(pool, "auto_backup_initialized").await.is_some() {
        return Ok(());
    }
    set_setting(pool, "auto_backup_initialized", "1").await?;

    // Respect a folder the user somehow already has (e.g. a DB from an older
    // build that predates the marker).
    if get_setting(pool, "auto_backup_dir").await.is_some_and(|d| !d.trim().is_empty()) {
        return Ok(());
    }

    let dir = data_dir.join("backups");
    fs::create_dir_all(&dir)?;
    set_setting(pool, "auto_backup_dir", &dir.to_string_lossy()).await?;
    Ok(())
}

// The DB runs in WAL mode: recent writes live in data.db-wal, not in data.db.
// Copying the file alone is not enough — the snapshot would be incomplete.
// VACUUM INTO writes a consistent copy of the whole DB (WAL included) to a
// separate file.
pub async fn export_impl(pool: &sqlx::SqlitePool, data_dir: &Path, path: &str) -> AppResult<()> {
    let snapshot_path = data_dir.join("data.db.export");

    let _ = std::fs::remove_file(&snapshot_path); // VACUUM INTO requires the file to be absent
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

// data.db must not be overwritten while the pool is live: the activity loop
// writes to the DB every 60 seconds and would clobber the import. We drop a
// staging file and restart the app — apply_pending_import() picks it up before
// the pool is opened.
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

// Decides whether an automatic backup is due: at least 24h since the last one
// and a folder is configured.
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
        None => true, // never run before — it is due
    }
}

// Performs an automatic backup: export plus rotation. Returns the filename.
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

    // Rotation: delete the oldest files beyond keep
    let mut entries: Vec<_> = fs::read_dir(&dir)?
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

// The failure path of the automatic backup, kept apart from auto_backup_impl so
// that one stays testable on its own.
//
// Why this exists (v0.9.86): the rotation step above used to be
// `read_dir(&dir).unwrap_or_else(|_| panic!(...))`. The loop that calls it runs
// inside tokio::spawn (lib.rs), and a panic in a spawned task does not bring the
// app down — it kills only that task. Backups would stop forever while the app
// went on looking perfectly healthy. That is the same shape of silent failure as
// the empty auto_backup_dir fixed in v0.9.85, so the error has to be recorded
// where the user can see it.
//
// No catch_unwind here on purpose: it would be armour around the one panic we
// just removed, and it would hide the next one just as well.
pub async fn run_auto_backup(pool: &sqlx::SqlitePool, data_dir: &Path) -> AppResult<String> {
    match auto_backup_impl(pool, data_dir).await {
        Ok(filename) => {
            // Cleared on success, otherwise a single bad run would leave a
            // warning in Settings for good.
            let _ = set_setting(pool, "last_auto_backup_error", "").await;
            Ok(filename)
        }
        Err(e) => {
            let record = format!("{}\t{}", chrono::Utc::now().to_rfc3339(), e);
            let _ = set_setting(pool, "last_auto_backup_error", &record).await;
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn do_auto_backup(
    app: tauri::AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    run_auto_backup(pool.inner(), &data_dir)
        .await
        .map_err(|e| e.to_string())
}

pub fn apply_pending_import(data_dir: &std::path::Path) {
    let staging = data_dir.join("data.db.import");
    if staging.exists() {
        let _ = std::fs::rename(&staging, data_dir.join("data.db"));
        // Otherwise WAL leftovers from the old DB would be replayed over the
        // imported one and silently roll the import back.
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

    // Not sqlite::memory: — with a pool over an in-memory DB every connection
    // sees its own empty database and VACUUM INTO may end up in the wrong place.
    // A file-backed DB matches production.
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

    // The full cycle: export to zip -> import into staging -> apply on "restart"
    // -> open the imported DB and check the data.
    #[tokio::test]
    async fn export_import_round_trip() {
        let dir = tmp_dir("roundtrip");
        let pool = test_pool(&dir).await;
        insert_task(&pool, "задача для бэкапа").await;

        let zip_path = dir.join("backup.zip");
        export_impl(&pool, &dir, zip_path.to_str().unwrap()).await.unwrap();
        assert!(zip_path.exists());
        // the temporary VACUUM INTO snapshot has been cleaned up
        assert!(!dir.join("data.db.export").exists());

        import_impl(&dir, zip_path.to_str().unwrap()).unwrap();
        assert!(dir.join("data.db.import").exists());

        // Simulate the pre-restart state: an old DB with WAL leftovers
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
        // no staging file must appear
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

        // a recent backup — must not trigger
        let recent = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        set_setting(&pool, "last_auto_backup", &recent).await.unwrap();
        assert!(!auto_backup_due(&pool).await);

        // 25 hours ago — must trigger
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

        // Unrelated files are left alone
        std::fs::write(backup_dir.join("note.txt"), b"hello").unwrap();

        // Create "old" backups via export_impl directly, not auto_backup_impl
        let pool = test_pool(&dir).await;
        set_setting(&pool, "auto_backup_dir", backup_dir.to_str().unwrap()).await.unwrap();
        set_setting(&pool, "auto_backup_keep", "3").await.unwrap();

        // Simulate 4 old backups
        for i in 1..=4 {
            let name = format!("ai-notes-backup-2026-07-{:02}0-1200.zip", i);
            std::fs::write(backup_dir.join(&name), b"fake-zip").unwrap();
        }

        // Run the automatic backup: it creates a new one and prunes the old
        auto_backup_impl(&pool, &dir).await.unwrap();

        // 3 backups (keep) + 1 unrelated file = 4 files must remain
        let mut entries: Vec<_> = std::fs::read_dir(&backup_dir).unwrap()
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let backup_count = entries.iter().filter(|e| {
            e.file_name().to_string_lossy().starts_with("ai-notes-backup-")
        }).count();
        assert_eq!(backup_count, 3, "должно быть 3 бэкапа после ротации");

        // The unrelated file is untouched
        assert!(backup_dir.join("note.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // v0.9.86. Until then the rotation step did
    // `read_dir(&dir).unwrap_or_else(|_| panic!(...))`, and the caller is a
    // tokio::spawn loop: the panic killed that task alone and automatic backups
    // stopped for good without a word anywhere.
    //
    // The failure has to land on read_dir specifically. A file where a directory
    // is expected — the obvious choice — is useless: export_impl fails first when
    // it cannot create the zip, so the test passes with the panic still in place
    // and proves nothing. Verified by trying exactly that.
    //
    // Write+execute without read is the one mode where the export succeeds and
    // the rotation cannot list the folder. Unix-only, hence the cfg.
    //
    // The assertion is literally that the call returns instead of panicking: a
    // panic would take the whole test binary down, which is the production
    // behaviour being fixed.
    #[cfg(unix)]
    #[tokio::test]
    async fn auto_backup_records_the_error_instead_of_panicking() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tmp_dir("backup-error");
        let pool = test_pool(&dir).await;

        let unreadable = dir.join("unreadable");
        std::fs::create_dir_all(&unreadable).unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o300)).unwrap();
        set_setting(&pool, "auto_backup_dir", unreadable.to_str().unwrap()).await.unwrap();

        let result = run_auto_backup(&pool, &dir).await;
        assert!(result.is_err(), "сбойный бэкап вернул Ok");
        // Proof the failure is the rotation step and not something earlier: with
        // the folder made readable again, the exported zip is there. Without this
        // the test would pass with the panic still in place — exactly the trap
        // hit on the first attempt at this test.
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let written = std::fs::read_dir(&unreadable).unwrap().count();
        assert_eq!(
            written, 1,
            "экспорт не дошёл до ротации — тест бьёт не в ту строку"
        );

        let recorded = get_setting(&pool, "last_auto_backup_error").await.unwrap_or_default();
        assert!(!recorded.is_empty(), "ошибка бэкапа нигде не записана");
        assert!(
            recorded.contains('\t'),
            "ожидался формат <rfc3339>\\t<сообщение>, получено: {recorded}"
        );

        // The other half: one bad run must not leave a warning in Settings for
        // good, so a later success clears it.
        let good_dir = dir.join("backups");
        std::fs::create_dir_all(&good_dir).unwrap();
        set_setting(&pool, "auto_backup_dir", good_dir.to_str().unwrap()).await.unwrap();

        run_auto_backup(&pool, &dir).await.unwrap();
        assert_eq!(
            get_setting(&pool, "last_auto_backup_error").await.unwrap_or_default(),
            "",
            "успешный бэкап не сбросил прошлую ошибку"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // v0.9.85: automatic backups are on out of the box. Until then auto_backup_dir
    // defaulted to empty and auto_backup_due refused to run — the live database of
    // the app's only daily user had never been backed up once.
    #[tokio::test]
    async fn default_backup_dir_is_set_on_a_fresh_db() {
        let dir = tmp_dir("default-backup");
        let pool = test_pool(&dir).await;

        assert_eq!(get_setting(&pool, "auto_backup_dir").await, None);
        ensure_default_backup_dir(&pool, &dir).await.unwrap();

        let set = get_setting(&pool, "auto_backup_dir").await.unwrap();
        assert_eq!(set, dir.join("backups").to_string_lossy());
        assert!(dir.join("backups").is_dir(), "каталог бэкапов не создан");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // The other half, and the reason for the marker key: clearing the folder in
    // Settings is a deliberate "turn it off". Re-seeding it on the next launch
    // would make the setting impossible to switch off.
    #[tokio::test]
    async fn a_deliberately_cleared_folder_is_not_restored() {
        let dir = tmp_dir("cleared-backup");
        let pool = test_pool(&dir).await;

        ensure_default_backup_dir(&pool, &dir).await.unwrap();
        set_setting(&pool, "auto_backup_dir", "").await.unwrap();

        ensure_default_backup_dir(&pool, &dir).await.unwrap();
        assert_eq!(
            get_setting(&pool, "auto_backup_dir").await.unwrap(),
            "",
            "очищенная пользователем папка вернулась при следующем запуске"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
