use serde::Serialize;

// Единый тип ошибок приложения. Tauri-команды возвращают Result<T, AppError>;
// на фронт ошибка уходит как строка (Serialize через Display).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Ошибка базы данных: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Ошибка файловой системы: {0}")]
    Io(#[from] std::io::Error),

    #[error("Ошибка архива: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Ошибка запроса к ИИ: {0}")]
    Http(#[from] reqwest::Error),

    #[error("{0}")]
    Tauri(#[from] tauri::Error),

    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Other(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
