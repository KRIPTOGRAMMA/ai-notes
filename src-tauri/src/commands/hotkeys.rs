// Rebinding the global quick-capture hotkeys.
//
// The four combinations (Ctrl+Shift+N/M/B/J) used to be hardcoded in lib.rs as
// string literals, while every webview hotkey had long been rebindable. The
// asymmetry was not obvious: the user opens the "Hotkeys" tab, does not find
// the global ones there and cannot tell whether they can be changed at all.
//
// The combination format is the same as for webview hotkeys (`Ctrl+Shift+KeyN`,
// i.e. `KeyboardEvent.code`). It was specifically verified that
// `Shortcut::parse` from global-hotkey understands "KeyN", "N" and "Digit1"
// alike: no converter between two formats is needed, and one format across the
// whole app means recording a combination in the UI works identically for both
// kinds of hotkey.
use crate::error::AppResult;
use serde::Serialize;

// An action that a global hotkey can launch. The list is closed: a global
// hotkey intercepts keys from the entire system, so "add your own action" here
// is not the same thing as configuring a hotkey inside the window.
pub struct GlobalAction {
    pub id: &'static str,
    pub label: &'static str,
    pub default_combo: &'static str,
    // The quick-capture window mode — the same set of strings as quick_mode.
    pub mode: &'static str,
}

pub const GLOBAL_ACTIONS: &[GlobalAction] = &[
    GlobalAction { id: "quick_task", label: "Быстрая задача", default_combo: "Ctrl+Shift+KeyN", mode: "task" },
    GlobalAction { id: "quick_note", label: "Быстрая заметка", default_combo: "Ctrl+Shift+KeyM", mode: "note" },
    // Not V: Ctrl+Shift+V is taken almost everywhere by "paste without formatting".
    GlobalAction { id: "quick_clip", label: "Заметка из буфера", default_combo: "Ctrl+Shift+KeyB", mode: "clipboard" },
    // J stands for "jump" — to the pinned item.
    GlobalAction { id: "quick_pinned", label: "Быстрый слот", default_combo: "Ctrl+Shift+KeyJ", mode: "pinned" },
];

// The same list for the frontend. Served by a command rather than duplicated in
// TS: this list is the source of truth for registration and must not drift.
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

// Parses overrides from settings.global_keybinds (JSON {action_id: combo}).
// Junk and empty values are ignored silently: the settings may have been edited
// by hand, and crashing over that is not acceptable — the only thing worse than
// a broken hotkey is an app that will not start because of one.
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

// The final "combination -> mode" list to be registered.
//
// Duplicates are dropped on a first-wins basis: the OS cannot tell two commands
// on one combination apart anyway, and silently handing it to the second action
// is worse than leaving it with the first in list order. The UI does not let
// such a conflict be created, but the settings can also be edited by hand.
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

// Validates a combination before saving. Returns an error with human-readable
// text rather than a bool: the reason for the refusal ("could not parse" /
// "taken by the system") matters more to the user than the bare fact.
//
// Checked on the backend rather than in TS: global-hotkey is what will parse the
// combination anyway, so its opinion is the only one that counts. A private copy
// of the rules in the frontend would one day drift from the real parser.
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
    // A lone key with no modifiers must not become a global hotkey: it would
    // capture that letter across the whole system, including any input field.
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
        // The main guard against a typo in a constant: a default that does not
        // parse would mean a hotkey silently unregistered at startup.
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
        // Non-string and empty values are dropped, the rest is kept.
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
        // the rest stay on their defaults
        assert!(binds.contains(&("Ctrl+Shift+KeyN".to_string(), "task")));
        assert!(!binds.iter().any(|(c, _)| c == "Ctrl+Shift+KeyJ"));
    }

    // One combination for two actions: the OS cannot tell them apart. We check
    // that exactly one pair is registered, not two with an unpredictable outcome.
    #[test]
    fn duplicate_combo_registered_once_first_wins() {
        let mut o = std::collections::HashMap::new();
        o.insert("quick_note".to_string(), "Ctrl+Shift+KeyN".to_string());
        let binds = resolve_bindings(&o);
        let n: Vec<_> = binds.iter().filter(|(c, _)| c == "Ctrl+Shift+KeyN").collect();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].1, "task"); // quick_task comes first in the list
        assert_eq!(binds.len(), GLOBAL_ACTIONS.len() - 1);
    }

    #[test]
    fn validate_rejects_empty_bare_key_and_garbage() {
        assert!(validate_combo("").is_err());
        assert!(validate_combo("   ").is_err());
        // without a modifier it would capture the letter across the system
        assert!(validate_combo("KeyN").is_err());
        assert!(validate_combo("Ctrl+ЧтоТо").is_err());
        assert!(validate_combo("Ctrl+Shift+KeyN").is_ok());
        // the global-hotkey format is accepted too — both variants are valid
        assert!(validate_combo("Ctrl+Shift+N").is_ok());
        assert!(validate_combo("Ctrl+Digit1").is_ok());
    }
}
