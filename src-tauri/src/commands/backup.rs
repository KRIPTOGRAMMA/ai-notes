use std::fs::File;
use std::io::{Read, Write};
use zip::ZipWriter;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use tauri::Manager;
use crate::error::AppResult;

#[tauri::command]
pub async fn export(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let data_dir = app.path().app_data_dir()?;
    let db_path = data_dir.join("data.db");

    let zip_file = File::create(&path)?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    zip.start_file("data.db", options)?;
    let mut db_file = File::open(&db_path)?;
    let mut buf = Vec::new();
    db_file.read_to_end(&mut buf)?;
    zip.write_all(&buf)?;

    zip.finish()?;
    Ok(())
}

// Нельзя перезаписывать data.db на живом пуле: activity-loop пишет в БД
// каждые 60 сек и затёр бы импорт. Кладём staging-файл и перезапускаем
// приложение — apply_pending_import() подхватит его до открытия пула.
#[tauri::command]
pub async fn import(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let data_dir = app.path().app_data_dir()?;
    let staging_path = data_dir.join("data.db.import");

    let zip_file = File::open(&path)?;
    let mut archive = ZipArchive::new(zip_file)?;

    let mut entry = archive.by_name("data.db")?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;

    std::fs::write(&staging_path, &buf)?;
    app.restart()
}

pub fn apply_pending_import(data_dir: &std::path::Path) {
    let staging = data_dir.join("data.db.import");
    if staging.exists() {
        let _ = std::fs::rename(&staging, data_dir.join("data.db"));
    }
}
