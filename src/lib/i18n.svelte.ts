// The reactive localization wrapper. State only — all the translation logic lives
// in the pure i18n.ts, which is covered by tests.
import { translate, detectLang, type Lang } from "./i18n";

let current = $state<Lang>("ru");

export const i18n = {
  get lang() { return current; },

  set(lang: Lang) { current = lang; },

  // The language from the settings; an empty string (the setting is unset) means we
  // detect it from the system locale. Called once at application startup.
  init(saved: string) {
    current = saved === "ru" || saved === "en"
      ? saved
      : detectLang(typeof navigator !== "undefined" ? navigator.language : "");
  },
};

// A short name for the markup: {t("Задачи")}. It reads i18n.lang, so components
// re-render on a language change by themselves.
export function t(key: string, vars?: Record<string, string | number>): string {
  return translate(key, i18n.lang, vars);
}
