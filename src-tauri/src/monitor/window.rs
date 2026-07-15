// Провайдеры активного окна: кто сейчас в фокусе (класс + заголовок).
//
// Принцип v0.5 — capability detection: detect_provider() пробует провайдеры
// по очереди и возвращает первый рабочий; ни одного — трекинг приложений
// просто выключен (activity_log.app = NULL), без ошибок и настроек.
//
// Реализовано: Hyprland (IPC-сокет). Кандидаты на будущее: X11 (_NET_ACTIVE_WINDOW),
// Windows (GetForegroundWindow) — добавляются как новые ветки в detect_provider().

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub app: String,   // класс окна (например "kitty", "zen")
    pub title: String, // заголовок — в БД не пишем, но полезен для будущих правил
}

pub trait WindowProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn current_window(&self) -> Option<WindowInfo>;
}

// Путь IPC-сокета Hyprland. Чистая функция — тестируется без окружения.
#[cfg(unix)]
fn hypr_socket_path(runtime_dir: &str, signature: &str) -> PathBuf {
    PathBuf::from(runtime_dir).join("hypr").join(signature).join(".socket.sock")
}

// Ответ `j/activewindow` — JSON с полями class/title (и десятком других).
// Пустой объект/не-JSON/отсутствующий class — нет активного окна.
#[cfg(unix)]
fn parse_active_window(raw: &str) -> Option<WindowInfo> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let app = v.get("class")?.as_str()?.trim().to_string();
    if app.is_empty() {
        return None;
    }
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    Some(WindowInfo { app, title })
}

#[cfg(unix)]
pub struct HyprlandProvider {
    socket: PathBuf,
}

#[cfg(unix)]
impl HyprlandProvider {
    pub fn detect() -> Option<Self> {
        let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
        let socket = hypr_socket_path(&runtime, &sig);
        socket.exists().then_some(Self { socket })
    }
}

#[cfg(unix)]
impl WindowProvider for HyprlandProvider {
    fn name(&self) -> &'static str {
        "Hyprland"
    }

    // Один запрос — одно соединение: так работает протокол hyprctl.
    // Сокет локальный, вызов на тике раз в log_interval_secs — цена нулевая.
    fn current_window(&self) -> Option<WindowInfo> {
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        stream.write_all(b"j/activewindow").ok()?;
        let mut raw = String::new();
        stream.read_to_string(&mut raw).ok()?;
        parse_active_window(&raw)
    }
}

pub fn detect_provider() -> Option<std::sync::Arc<dyn WindowProvider>> {
    #[cfg(unix)]
    if let Some(p) = HyprlandProvider::detect() {
        return Some(std::sync::Arc::new(p));
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn socket_path_layout() {
        assert_eq!(
            hypr_socket_path("/run/user/1000", "abc123"),
            PathBuf::from("/run/user/1000/hypr/abc123/.socket.sock")
        );
    }

    #[test]
    fn parses_class_and_title() {
        let raw = r#"{"address":"0x1","class":"kitty","title":"~/Projects","pid":42}"#;
        assert_eq!(
            parse_active_window(raw),
            Some(WindowInfo { app: "kitty".into(), title: "~/Projects".into() })
        );
    }

    #[test]
    fn no_active_window_variants() {
        assert_eq!(parse_active_window("{}"), None); // пустой рабочий стол
        assert_eq!(parse_active_window("Invalid"), None); // не-JSON ответ
        assert_eq!(parse_active_window(r#"{"class":""}"#), None); // пустой класс
        assert_eq!(parse_active_window(r#"{"class":"x"}"#).unwrap().title, ""); // без title
    }
}
