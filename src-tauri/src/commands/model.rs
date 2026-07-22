use std::io::Write;
use std::path::PathBuf;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

// Рекомендуемая по умолчанию модель — маленькая инструктивная GGUF, тянет на CPU.
// Поле URL в UI редактируемое: можно подставить любую другую GGUF.
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf";

#[derive(Clone, Serialize)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    pub description: String,
    pub ram_gb: u8,
    pub recommended: bool,
}

// Курируемый список (v0.9.07) — только реальные GGUF-квантизации с HuggingFace,
// проверенные вручную (не выдуманные URL/размеры). size_bytes — размер файла
// квантизации на HF на момент добавления списка (может незначительно
// отличаться от актуального, если автор перезалил файл — не критично, это
// ориентир для пользователя перед скачиванием, а не проверка целостности).
// download_model()/model_status() не меняются: любая модель из списка (или
// вручную вставленный URL, поле остаётся редактируемым) кладётся под тем же
// именем model.gguf — sidecar.rs не завязан на конкретную модель.
pub fn model_catalog() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "qwen2.5-0.5b".into(),
            name: "Qwen2.5 0.5B Instruct".into(),
            url: DEFAULT_MODEL_URL.into(),
            size_bytes: 491_000_000,
            description: "Самая быстрая и лёгкая — годится для слабых машин и старых ноутбуков, но качество ответов базовое.".into(),
            ram_gb: 2,
            recommended: false,
        },
        ModelOption {
            id: "qwen2.5-1.5b".into(),
            name: "Qwen2.5 1.5B Instruct".into(),
            url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf".into(),
            size_bytes: 1_120_000_000,
            description: "Баланс скорости и качества — заметно лучше 0.5B в рассуждениях, всё ещё быстрая на CPU.".into(),
            ram_gb: 3,
            recommended: true,
        },
        ModelOption {
            id: "phi-3.5-mini".into(),
            name: "Phi-3.5 Mini Instruct".into(),
            url: "https://huggingface.co/bartowski/Phi-3.5-mini-instruct-GGUF/resolve/main/Phi-3.5-mini-instruct-Q4_K_M.gguf".into(),
            size_bytes: 2_390_000_000,
            description: "Лучшее качество из трёх — точнее держит инструкции и контекст, но медленнее и требует больше памяти.".into(),
            ram_gb: 5,
            recommended: false,
        },
    ]
}

#[tauri::command]
pub fn list_model_options() -> Vec<ModelOption> {
    model_catalog()
}

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

// Есть ли скачанная локальная модель. Без создания каталога — только проверка.
pub(crate) fn local_model_available(app: &AppHandle) -> bool {
    app.path()
        .app_data_dir()
        .map(|d| d.join("models").join("model.gguf").exists())
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_ids_are_unique_and_non_empty() {
        let catalog = model_catalog();
        assert!(!catalog.is_empty());
        let ids: HashSet<&str> = catalog.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), catalog.len());
    }

    #[test]
    fn catalog_has_exactly_one_recommended_entry() {
        let recommended = model_catalog().into_iter().filter(|m| m.recommended).count();
        assert_eq!(recommended, 1);
    }

    #[test]
    fn catalog_contains_default_model_url_entry() {
        let catalog = model_catalog();
        assert!(catalog.iter().any(|m| m.url == DEFAULT_MODEL_URL));
    }

    #[test]
    fn catalog_urls_are_https_and_unique() {
        let catalog = model_catalog();
        let urls: HashSet<&str> = catalog.iter().map(|m| m.url.as_str()).collect();
        assert_eq!(urls.len(), catalog.len());
        assert!(catalog.iter().all(|m| m.url.starts_with("https://")));
    }
}
