// Interface localization — the pure part.
//
// A minimal layer of our own instead of svelte-i18n: there are two languages, the
// interpolation needed is simple, and the library brings its own store, async
// locale loading and the ICU format — weight with no benefit here. Besides, a
// dictionary that is a plain object can be checked for completeness by a test,
// which lazy loading rules out.
//
// The reactive state (the current language) lives in i18n.svelte.ts: runes work
// only in .svelte.ts, and vitest in this project covers pure .ts only — the same
// split as guard.ts and clipboardNote.ts.
//
// Russian is the source language: the keys match the Russian text word for word.
// That is deliberate. Keys like `tasks.empty_state` would mean rewriting some 650
// lines of markup blind and losing diff readability. With text-as-key a missing
// translation degrades into a Russian string rather than into "tasks.empty_state"
// on screen.

import { EN } from "./i18n.en";

export type Lang = "ru" | "en";

export const LANGS: { id: Lang; label: string }[] = [
  { id: "ru", label: "Русский" },
  { id: "en", label: "English" },
];

// The language on first launch, before the user has chosen explicitly. Anything
// not Russian counts as English: to a non-Russian user English is more useful than
// Russian, and the converse does not hold.
export function detectLang(nav: string): Lang {
  return nav.toLowerCase().startsWith("ru") ? "ru" : "en";
}

/**
 * Translates a string. `translate("Задачи", "en")` yields "Tasks".
 *
 * Substitution uses `{name}`: translate("Удалено {n}", "en", { n: 3 }).
 * A missing translation returns the key (the Russian original) rather than an
 * empty string or "MISSING": an unfinished translation must not break the screen.
 */
export function translate(
  key: string,
  lang: Lang,
  vars?: Record<string, string | number>,
): string {
  let out = lang === "en" ? EN[key] ?? key : key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      out = out.split(`{${k}}`).join(String(v));
    }
  }
  return out;
}

// --- Seeded categories and statuses ---
//
// Categories (migration 0015) and statuses (0029) are rows in the DB rather than
// code: the user creates, renames and deletes them. So a name from the table must
// not be translated — a category the user called "Работа" has to stay "Работа" in
// any language: that is their text, not ours.
//
// The exception is the seeded rows: we wrote their names, in a migration, and they
// are as much part of the interface as the labels on buttons. They are recognized
// by the id-and-name pair rather than by the name alone: the user may create their
// own category named "Работа", and that one must stay Russian.
//
// The names are exactly as migrations 0015 and 0029 wrote them. A mismatch against
// this table means one of two things: the row is user-defined (its id is not here)
// or a seeded one was renamed. Either way it is someone else's text.
const SEEDED_ORIGINALS: Record<string, string> = {
  "category:Work": "Работа",
  "category:Study": "Учёба",
  "category:Home": "Дом",
  "category:Health": "Здоровье",
  "category:Other": "Другое",
  "status:Todo": "Todo",
  "status:InProgress": "В работе",
  "status:Done": "Готово",
  "status:Archived": "Архив",
};

// For the "Task categories" tab in Settings: categories have no is_reserved flag
// (unlike statuses), yet the rename field must be disabled for exactly the seeded
// ones — otherwise the translation would go into the DB over the original. Derived
// from the table above so the two lists cannot drift apart when one is edited.
export const SEEDED_CATEGORY_IDS = new Set(
  Object.keys(SEEDED_ORIGINALS)
    .filter(k => k.startsWith("category:"))
    .map(k => k.slice("category:".length)),
);

/**
 * The name of a seeded category or status in the interface language; for
 * user-defined and renamed ones, the name from the DB unchanged.
 *
 * `name` is checked against the seeded original: if the user renamed a seeded
 * category (keeping the same id), the translation no longer applies — otherwise
 * their edit would be invisible in English.
 */
export function seededName(
  kind: "category" | "status",
  id: string,
  name: string,
  lang: Lang,
): string {
  // One check suffices for uuid rows too: their id is absent from the table, and
  // `undefined !== name` rejects them just as it rejects a renamed seeded row.
  const original = SEEDED_ORIGINALS[`${kind}:${id}`];
  if (original !== name) return name;
  return translate(name, lang);
}
