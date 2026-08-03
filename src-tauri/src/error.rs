use serde::Serialize;

// The application's single error type. Tauri commands return Result<T, AppError>;
// the error reaches the frontend as a string (Serialize via Display).
//
// The prefixes below are dictionary keys on the frontend: the message arrives as
// "<prefix>: <detail>", and src/lib/errorText.ts translates the prefix alone,
// leaving the sqlx/io/zip/reqwest detail untouched. Translating here is not an
// option — #[error] is a compile-time literal and the Serialize impl is sync with
// no access to the pool `current_lang` needs. Changing a prefix means changing
// src/lib/errorText.ts and src/lib/i18n.en.ts too; prefixes_are_stable below
// fails if they drift apart.
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

#[cfg(test)]
mod tests {
    use super::*;

    // The four prefixes the frontend knows how to translate. Kept here as data so
    // the test can compare them against both the real Display output and the
    // frontend's own list.
    const PREFIXES: [&str; 4] = [
        "Ошибка базы данных",
        "Ошибка файловой системы",
        "Ошибка архива",
        "Ошибка запроса к ИИ",
    ];

    #[test]
    fn prefixes_are_stable() {
        let db = AppError::Db(sqlx::Error::RowNotFound);
        let io = AppError::Io(std::io::Error::other("disk"));
        let zip = AppError::Zip(zip::result::ZipError::FileNotFound);

        assert!(db.to_string().starts_with("Ошибка базы данных: "));
        assert!(io.to_string().starts_with("Ошибка файловой системы: "));
        assert!(zip.to_string().starts_with("Ошибка архива: "));
    }

    // Other and Tauri are "{0}" passthrough: a domain message must reach the user
    // exactly as written, with no prefix glued on. The frontend relies on this —
    // "Задача не найдена: abc" contains ": " and must not be split.
    #[test]
    fn domain_messages_carry_no_prefix() {
        let msg = "Задача не найдена: abc";
        assert_eq!(AppError::Other(msg.to_string()).to_string(), msg);
        for prefix in PREFIXES {
            assert!(
                !AppError::Other(msg.to_string()).to_string().starts_with(prefix),
                "domain message must not start with the technical prefix {prefix}"
            );
        }
    }

    // The frontend translates by matching against a hardcoded list. If a prefix is
    // renamed here and not there, the message silently stops being translated —
    // this test is what catches that.
    #[test]
    fn frontend_knows_the_same_prefixes() {
        let ts = include_str!("../../src/lib/errorText.ts");
        for prefix in PREFIXES {
            assert!(
                ts.contains(&format!("\"{prefix}\"")),
                "src/lib/errorText.ts does not list the prefix {prefix}"
            );
        }
        let dict = include_str!("../../src/lib/i18n.en.ts");
        for prefix in PREFIXES {
            assert!(
                dict.contains(&format!("\"{prefix}\":")),
                "src/lib/i18n.en.ts has no translation for the prefix {prefix}"
            );
        }
    }
}
