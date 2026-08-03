// Date and time formatting shared by the screens.
//
// A pure module on purpose, like guard.ts: vitest in this project is configured
// for plain ts only (vitest.config.ts, include: src/**/*.test.ts, no svelte
// plugin), so anything living next to a $state rune cannot be unit-tested. The
// UI language is passed in as an argument instead of being read from the i18n
// rune, which is what keeps these functions testable.
//
// The rule these functions exist to enforce: a calendar day shown to the user is
// a LOCAL day. toISOString() returns UTC and silently shifts the date by the
// timezone offset — just after midnight it reports yesterday.

import type { Lang } from "./i18n";

/** "07" — the two-digit pad every time format here is built from. */
export function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/** "09:05" — wall-clock time of a Date, local. */
export function hhmm(d: Date): string {
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

/** "09:05" from minutes since midnight (routines store start_mins this way). */
export function hhmmFromMins(mins: number): string {
  return `${pad2(Math.floor(mins / 60))}:${pad2(mins % 60)}`;
}

/** "2026-08-03" — the local calendar day. Deliberately not toISOString(). */
export function localDateKey(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** "2026-08-03T09:05" — the value shape <input type="datetime-local"> expects. */
export function toLocalInput(iso: string): string {
  const d = new Date(iso);
  return `${localDateKey(d)}T${hhmm(d)}`;
}

/**
 * Elapsed or remaining time: "4:09", or "1:04:09" once it passes an hour.
 * `alwaysMinutes` keeps the minute:second shape past the hour mark ("65:00"),
 * which is what the pomodoro widget has always shown.
 */
export function duration(totalSecs: number, alwaysMinutes = false): string {
  const secs = Math.max(0, totalSecs);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0 && !alwaysMinutes) return `${h}:${pad2(m)}:${pad2(s)}`;
  return `${alwaysMinutes ? Math.floor(secs / 60) : m}:${pad2(s)}`;
}

/** The Intl locale tag for a UI language. */
export function localeTag(lang: Lang): string {
  return lang === "en" ? "en-US" : "ru-RU";
}
