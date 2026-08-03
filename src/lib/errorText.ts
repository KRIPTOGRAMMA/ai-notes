// Translating the technical prefix of a backend error.
//
// AppError (src-tauri/src/error.rs) renders as "<Russian prefix>: <detail>",
// where the prefix names the failure class and the detail is raw sqlx/io/zip/
// reqwest text. The prefix is ours and belongs in the dictionary; the detail is
// the library's and stays as it is — translating it is neither possible nor
// useful for reporting a problem.
//
// The prefix cannot be translated on the Rust side: thiserror's #[error] is a
// compile-time literal, and AppError's Serialize impl is synchronous with no
// access to the pool that `current_lang` needs.
//
// A pure module, like guard.ts and datetime.ts: the translator comes in as an
// argument rather than from the i18n rune, which keeps it unit-testable.

// Closed set on purpose. Domain messages carry ": " too — "Недопустимое
// расширение: png", "Некорректный base64: ..." — so splitting on the first ": "
// and translating whatever comes before it would mangle them. Only these four
// heads are ours to touch; anything else is returned untouched.
//
// Must stay in sync with the #[error] attributes in src-tauri/src/error.rs; the
// prefixes_are_stable test there guards that side.
const BACKEND_PREFIXES = [
  "Ошибка базы данных",
  "Ошибка файловой системы",
  "Ошибка архива",
  "Ошибка запроса к ИИ",
];

export function localizeBackendError(msg: string, tr: (key: string) => string): string {
  for (const prefix of BACKEND_PREFIXES) {
    if (msg.startsWith(prefix + ": ")) {
      return tr(prefix) + msg.slice(prefix.length);
    }
  }
  return msg;
}
