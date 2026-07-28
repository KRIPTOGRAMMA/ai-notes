// Реактивная обёртка локализации (v0.9.32). Здесь только состояние —
// вся логика перевода в чистом i18n.ts, который покрыт тестами.
import { translate, detectLang, type Lang } from "./i18n";

let current = $state<Lang>("ru");

export const i18n = {
  get lang() { return current; },

  set(lang: Lang) { current = lang; },

  // Язык из настроек; пустая строка (настройка не задана) — определяем по
  // системной локали. Вызывается один раз при старте приложения.
  init(saved: string) {
    current = saved === "ru" || saved === "en"
      ? saved
      : detectLang(typeof navigator !== "undefined" ? navigator.language : "");
  },
};

// Короткое имя для разметки: {t("Задачи")}. Читает i18n.lang, поэтому
// компоненты перерисовываются при смене языка сами.
export function t(key: string, vars?: Record<string, string | number>): string {
  return translate(key, i18n.lang, vars);
}
