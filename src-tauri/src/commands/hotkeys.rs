// v0.9.35: переназначение глобальных хоткеев быстрого ввода.
//
// До этой версии четыре комбинации (Ctrl+Shift+N/M/B/J) были зашиты в lib.rs
// строковыми литералами, тогда как все webview-хоткеи переназначались с
// v0.8.9. Асимметрия неочевидная: пользователь видит вкладку «Хоткеи», не
// находит там глобальных и не понимает, можно их менять или нет.
//
// Формат комбинации — тот же, что у webview-хоткеев (`Ctrl+Shift+KeyN`,
// `KeyboardEvent.code`). Специально проверено, что `Shortcut::parse` из
// global-hotkey понимает и "KeyN", и "N", и "Digit1": конвертер между двумя
// форматами не нужен, а один формат на всё приложение означает, что запись
// комбинации в UI одна и та же для обоих видов хоткеев.
use crate::error::AppResult;
use serde::Serialize;

// Действие, которое умеет запускаться глобальным хоткеем. Список закрытый:
// глобальный хоткей перехватывает клавиши у всей системы, поэтому «добавить
// своё действие» здесь — не то же самое, что настроить хоткей внутри окна.
pub struct GlobalAction {
    pub id: &'static str,
    pub label: &'static str,
    pub default_combo: &'static str,
    // Режим окна быстрого ввода — тот же набор строк, что у quick_mode.
    pub mode: &'static str,
}

pub const GLOBAL_ACTIONS: &[GlobalAction] = &[
    GlobalAction { id: "quick_task", label: "Быстрая задача", default_combo: "Ctrl+Shift+KeyN", mode: "task" },
    GlobalAction { id: "quick_note", label: "Быстрая заметка", default_combo: "Ctrl+Shift+KeyM", mode: "note" },
    // Не V: Ctrl+Shift+V почти везде занят «вставить без форматирования».
    GlobalAction { id: "quick_clip", label: "Заметка из буфера", default_combo: "Ctrl+Shift+KeyB", mode: "clipboard" },
    // J — от «jump», к закреплённому (v0.9.33).
    GlobalAction { id: "quick_pinned", label: "Быстрый слот", default_combo: "Ctrl+Shift+KeyJ", mode: "pinned" },
];

// То же самое для фронта. Отдаётся командой, а не дублируется в TS: список
// действий — источник правды для регистрации, и разъехаться он не должен.
#[derive(Debug, Serialize, PartialEq)]
pub struct GlobalActionInfo {
    pub id: String,
    pub label: String,
    pub default_combo: String,
}

#[tauri::command]
pub fn list_global_actions() -> Vec<GlobalActionInfo> {
    GLOBAL_ACTIONS
        .iter()
        .map(|a| GlobalActionInfo {
            id: a.id.into(),
            label: a.label.into(),
            default_combo: a.default_combo.into(),
        })
        .collect()
}

// Разбор оверрайдов из settings.global_keybinds (JSON {action_id: combo}).
// Мусор и пустые значения игнорируются молча: настройки могли быть
// отредактированы руками, и падать из-за этого нельзя — хуже сломанного
// хоткея только приложение, которое из-за него не стартует.
pub fn parse_overrides(json: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };
    for (k, v) in map {
        if let Some(s) = v.as_str() {
            if !s.trim().is_empty() {
                out.insert(k, s.trim().to_string());
            }
        }
    }
    out
}

// Итоговый список «комбинация → режим», который нужно зарегистрировать.
//
// Дубли отбрасываются по принципу «первый выигрывает»: две команды на одной
// комбинации ОС всё равно не различит, и молча отдать её второму действию
// хуже, чем оставить первому по порядку списка. UI не даёт создать такой
// конфликт, но настройки правятся и руками.
pub fn resolve_bindings(overrides: &std::collections::HashMap<String, String>) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    for action in GLOBAL_ACTIONS {
        let combo = overrides
            .get(action.id)
            .map(|s| s.as_str())
            .unwrap_or(action.default_combo);
        if out.iter().any(|(c, _)| c == combo) {
            continue;
        }
        out.push((combo.to_string(), action.mode));
    }
    out
}

// Проверка комбинации перед сохранением. Возвращает ошибку с человеческим
// текстом, а не bool: причина отказа («не разобрал» / «занято системой»)
// пользователю важнее самого факта.
//
// Проверяется на бэкенде, а не в TS: разбирать комбинацию будет всё равно
// global-hotkey, и его же мнение — единственное, которое считается. Своя
// копия правил во фронте однажды разошлась бы с настоящим парсером.
#[tauri::command]
pub fn validate_global_combo(combo: String) -> AppResult<()> {
    validate_combo(&combo)
}

pub fn validate_combo(combo: &str) -> AppResult<()> {
    use tauri_plugin_global_shortcut::Shortcut;
    let combo = combo.trim();
    if combo.is_empty() {
        return Err("Пустая комбинация".to_string().into());
    }
    // Одинокая клавиша без модификаторов глобальным хоткеем быть не должна:
    // она перехватит эту букву во всей системе, включая любое поле ввода.
    if !combo.contains('+') {
        return Err("Нужен хотя бы один модификатор (Ctrl, Shift, Alt)".to_string().into());
    }
    combo
        .parse::<Shortcut>()
        .map_err(|_| format!("Не удалось разобрать комбинацию: {combo}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_as_real_shortcuts() {
        // Главная защита от опечатки в константе: дефолт, который не парсится,
        // означал бы хоткей, молча не зарегистрированный при старте.
        for a in GLOBAL_ACTIONS {
            validate_combo(a.default_combo)
                .unwrap_or_else(|e| panic!("дефолт {} не парсится: {e}", a.id));
        }
    }

    #[test]
    fn action_ids_and_defaults_are_unique() {
        let mut ids: Vec<&str> = GLOBAL_ACTIONS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "дублирующийся id действия");

        let mut combos: Vec<&str> = GLOBAL_ACTIONS.iter().map(|a| a.default_combo).collect();
        combos.sort_unstable();
        let before = combos.len();
        combos.dedup();
        assert_eq!(combos.len(), before, "два действия с одним дефолтом");
    }

    #[test]
    fn parse_overrides_survives_garbage() {
        assert!(parse_overrides("").is_empty());
        assert!(parse_overrides("не json").is_empty());
        assert!(parse_overrides("[1,2]").is_empty());
        // Нестроковые и пустые значения выкидываются, остальное остаётся.
        let m = parse_overrides(r#"{"quick_task":"Ctrl+Alt+KeyT","quick_note":"","quick_clip":5}"#);
        assert_eq!(m.get("quick_task").map(|s| s.as_str()), Some("Ctrl+Alt+KeyT"));
        assert_eq!(m.get("quick_note"), None);
        assert_eq!(m.get("quick_clip"), None);
    }

    #[test]
    fn resolve_uses_defaults_when_no_overrides() {
        let binds = resolve_bindings(&Default::default());
        assert_eq!(binds.len(), GLOBAL_ACTIONS.len());
        assert_eq!(binds[0], ("Ctrl+Shift+KeyN".to_string(), "task"));
        assert_eq!(binds[3], ("Ctrl+Shift+KeyJ".to_string(), "pinned"));
    }

    #[test]
    fn resolve_applies_override_and_keeps_mode() {
        let mut o = std::collections::HashMap::new();
        o.insert("quick_pinned".to_string(), "Alt+KeyP".to_string());
        let binds = resolve_bindings(&o);
        assert!(binds.contains(&("Alt+KeyP".to_string(), "pinned")));
        // остальные — на дефолтах
        assert!(binds.contains(&("Ctrl+Shift+KeyN".to_string(), "task")));
        assert!(!binds.iter().any(|(c, _)| c == "Ctrl+Shift+KeyJ"));
    }

    // Одна комбинация на два действия — ОС их не различит. Проверяем, что
    // регистрируется ровно одна пара, а не две с непредсказуемым исходом.
    #[test]
    fn duplicate_combo_registered_once_first_wins() {
        let mut o = std::collections::HashMap::new();
        o.insert("quick_note".to_string(), "Ctrl+Shift+KeyN".to_string());
        let binds = resolve_bindings(&o);
        let n: Vec<_> = binds.iter().filter(|(c, _)| c == "Ctrl+Shift+KeyN").collect();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].1, "task"); // первым в списке идёт quick_task
        assert_eq!(binds.len(), GLOBAL_ACTIONS.len() - 1);
    }

    #[test]
    fn validate_rejects_empty_bare_key_and_garbage() {
        assert!(validate_combo("").is_err());
        assert!(validate_combo("   ").is_err());
        // без модификатора — перехватило бы букву во всей системе
        assert!(validate_combo("KeyN").is_err());
        assert!(validate_combo("Ctrl+ЧтоТо").is_err());
        assert!(validate_combo("Ctrl+Shift+KeyN").is_ok());
        // формат global-hotkey тоже принимается — оба варианта живые
        assert!(validate_combo("Ctrl+Shift+N").is_ok());
        assert!(validate_combo("Ctrl+Digit1").is_ok());
    }
}
