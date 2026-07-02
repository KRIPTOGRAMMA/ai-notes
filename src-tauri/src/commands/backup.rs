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

#[tauri::command]
pub async fn import(app: tauri::AppHandle, path: String) -> AppResult<()> {
    let data_dir = app.path().app_data_dir()?;
    let db_path = data_dir.join("data.db");

    let zip_file = File::open(&path)?;
    let mut archive = ZipArchive::new(zip_file)?;

    let mut entry = archive.by_name("data.db")?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;

    std::fs::write(&db_path, &buf)?;
    Ok(())
}
