// Voice input: dictating notes instead of typing them.
//
// The pipeline is deliberately batch rather than streaming — record, stop,
// transcribe — because whisper.cpp is a one-shot command over a finished file,
// not a server like llamafile. A dictated phrase is seconds long, so waiting for
// the end costs little and the code stays a great deal simpler.

pub mod audio;

use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;

/// The sidecar's name as tauri-plugin-shell expects it: a bare name, not a path.
///
/// It is resolved as `<directory of the executable>/<name>`, so a directory prefix
/// (`binaries/`, the layout in the sources and in tauri.conf.json) points at a
/// subdirectory that does not exist beside the built binary — the process then
/// fails to start with "No such file or directory".
const SIDECAR: &str = "whisper-cli";

/// Strips whisper's own decorations from the transcript.
///
/// Even with -np -nt the model emits bracketed annotations for non-speech
/// ([BLANK_AUDIO], [MUSIC], (wind blowing)) and pads the text with whitespace.
/// Inserting those into a note would be worse than inserting nothing.
pub fn clean_transcript(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth_square = 0usize;
    let mut depth_round = 0usize;
    for ch in raw.chars() {
        match ch {
            '[' => depth_square += 1,
            ']' => depth_square = depth_square.saturating_sub(1),
            '(' => depth_round += 1,
            ')' => depth_round = depth_round.saturating_sub(1),
            _ if depth_square == 0 && depth_round == 0 => out.push(ch),
            _ => {}
        }
    }
    // Whitespace is collapsed because removing an annotation leaves a gap behind,
    // and whisper indents every segment anyway.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether voice input can run at all: both the model and the sidecar must exist.
///
/// The same capability detection the project already applies to window tracking
/// and notification actions — the button simply does not appear when dictation
/// cannot work, rather than appearing and failing.
pub fn is_available(app: &AppHandle) -> bool {
    crate::commands::model::model_available(app, crate::commands::model::ModelKind::Whisper)
        && app.shell().sidecar(SIDECAR).is_ok()
}

/// Runs whisper over a finished WAV and returns the recognised text.
///
/// The flags are chosen so stdout carries the transcript and nothing else:
/// `-np` suppresses the progress and system information, `-nt` the timestamps.
///
/// The language is left on `auto` rather than following the interface language:
/// the two are unrelated — the author of this app runs a Russian interface and
/// dictates in both languages — and forcing the wrong one produces confident
/// nonsense rather than an error (verified: forcing `-l ru` on English speech
/// returned a mangled half-translation).
pub async fn transcribe(app: &AppHandle, wav: &std::path::Path) -> Result<String, String> {
    let model = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(crate::commands::model::ModelKind::Whisper.file_name());

    if !model.exists() {
        return Err("Модель распознавания не найдена. Скачайте её в настройках.".into());
    }

    let output = app
        .shell()
        .sidecar(SIDECAR)
        .map_err(|e| format!("Не удалось найти whisper-cli: {e}"))?
        .args([
            "-m", model.to_str().ok_or("Некорректный путь к модели")?,
            "-f", wav.to_str().ok_or("Некорректный путь к записи")?,
            "-l", "auto",
            "-np",
            "-nt",
        ])
        .output()
        .await
        .map_err(|e| format!("Не удалось запустить распознавание: {e}"))?;

    if !output.status.success() {
        // whisper writes diagnostics to stderr; its last line is the useful part.
        let err = String::from_utf8_lossy(&output.stderr);
        let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("неизвестная ошибка");
        return Err(format!("Распознавание не удалось: {last}"));
    }

    let text = clean_transcript(&String::from_utf8_lossy(&output.stdout));
    if text.is_empty() {
        return Err("Не удалось разобрать речь — попробуйте ещё раз.".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // whisper marks silence with [BLANK_AUDIO]; pasting that into a note would be
    // worse than pasting nothing.
    #[test]
    fn bracketed_annotations_are_removed() {
        assert_eq!(clean_transcript(" [BLANK_AUDIO]"), "");
        assert_eq!(clean_transcript("[MUSIC] привет"), "привет");
        assert_eq!(clean_transcript("привет (wind blowing) мир"), "привет мир");
    }

    #[test]
    fn leading_whitespace_and_padding_go_away() {
        assert_eq!(clean_transcript("  Купить хлеб.  "), "Купить хлеб.");
        assert_eq!(clean_transcript("строка\n  вторая"), "строка вторая");
    }

    // Real speech must survive untouched — the cleaner must not eat punctuation or
    // ordinary words.
    #[test]
    fn plain_speech_is_preserved() {
        let s = "Позвонить Ивану в 15:00, обсудить бюджет — это важно!";
        assert_eq!(clean_transcript(s), s);
    }

    // The recognised text may legitimately contain brackets the user dictated; an
    // unbalanced one must not swallow the rest of the phrase.
    #[test]
    fn unbalanced_closing_bracket_does_not_swallow_the_text() {
        assert_eq!(clean_transcript("текст] дальше"), "текст дальше");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert_eq!(clean_transcript(""), "");
        assert_eq!(clean_transcript("   \n  "), "");
    }

    // The sidecar name is a name, not a path. tauri-plugin-shell builds the command
    // as `<directory of the executable>/<name>` (relative_command_path), so a name
    // carrying a directory prefix points at a subdirectory that does not exist
    // beside the binary, and the process fails to start with "No such file or
    // directory". The directory in tauri.conf.json is the layout in the sources;
    // beside the built application there is none.
    //
    // Found only by running the app — nothing in the suite covered it. The test
    // repeats the same path arithmetic. Matching against the source text is not an
    // option: such a check trips over its own comment (tried).
    #[test]
    fn sidecar_name_resolves_next_to_the_executable() {
        let exe_dir = std::path::Path::new("/opt/app");
        assert_eq!(exe_dir.join(SIDECAR), std::path::Path::new("/opt/app/whisper-cli"));
        assert_eq!(
            exe_dir.join("binaries/whisper-cli"),
            std::path::Path::new("/opt/app/binaries/whisper-cli"),
            "this is the path that did not exist"
        );
    }
}
