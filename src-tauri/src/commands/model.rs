use std::io::Write;
use std::path::PathBuf;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

// Рекомендуемая по умолчанию модель — маленькая инструктивная GGUF, тянет на CPU.
// Поле URL в UI редактируемое: можно подставить любую другую GGUF.
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub pct: u8,
}

#[derive(Serialize)]
pub struct ModelStatus {
    pub exists: bool,
    pub size_bytes: u64,
}

fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("models");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
pub fn default_model_url() -> String {
    DEFAULT_MODEL_URL.to_string()
}

#[tauri::command]
pub async fn model_status(app: AppHandle) -> Result<ModelStatus, String> {
    let path = models_dir(&app)?.join("model.gguf");
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(ModelStatus { exists: path.exists(), size_bytes })
}

#[tauri::command]
pub async fn download_model(app: AppHandle, url: String) -> Result<(), String> {
    let dir = models_dir(&app)?;
    let final_path = dir.join("model.gguf");
    // Качаем в .part и переименовываем только после полной загрузки — чтобы
    // прерванная закачка не выглядела как готовая модель.
    let part_path = dir.join("model.gguf.part");

    let mut resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Сервер вернул {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&part_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 255; // заведомо невозможный, чтобы первый эмит прошёл

    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if let Err(e) = file.write_all(&chunk) {
            let _ = std::fs::remove_file(&part_path);
            return Err(e.to_string());
        }
        downloaded += chunk.len() as u64;
        let pct = if total > 0 { ((downloaded * 100) / total).min(100) as u8 } else { 0 };
        if pct != last_pct {
            last_pct = pct;
            let _ = app.emit("model-download-progress", DownloadProgress { downloaded, total, pct });
        }
    }

    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
    let _ = app.emit("model-download-progress", DownloadProgress { downloaded, total, pct: 100 });
    Ok(())
}
