use std::fs::File;
use std::io::{Read, Write};
use zip::ZipWriter;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use tauri::Manager;

#[tauri::command]
pub async fn export(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("data.db");

    let zip_file = File::create(&path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default();

    zip.start_file("data.db", options).map_err(|e| e.to_string())?;
    let mut db_file = File::open(&db_path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    db_file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    zip.write_all(&buf).map_err(|e| e.to_string())?;

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn import(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = data_dir.join("data.db");

    let zip_file = File::open(&path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(zip_file).map_err(|e| e.to_string())?;

    let mut entry = archive.by_name("data.db").map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    std::fs::write(&db_path, &buf).map_err(|e| e.to_string())?;
    Ok(())
}