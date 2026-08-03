import { describe, it, expect } from "vitest";
import { localizeBackendError } from "./errorText";
import { translate } from "./i18n";

const en = (key: string) => translate(key, "en");
const ru = (key: string) => translate(key, "ru");

describe("localizeBackendError", () => {
  it("технический префикс переводится, детали остаются как есть", () => {
    const msg = "Ошибка базы данных: no such table: tasks";
    expect(localizeBackendError(msg, en)).toBe("Database error: no such table: tasks");
  });

  it("переводятся все четыре класса ошибок", () => {
    expect(localizeBackendError("Ошибка файловой системы: EACCES", en))
      .toBe("Filesystem error: EACCES");
    expect(localizeBackendError("Ошибка архива: invalid header", en))
      .toBe("Archive error: invalid header");
    expect(localizeBackendError("Ошибка запроса к ИИ: timeout", en))
      .toBe("AI request error: timeout");
  });

  it("по-русски строка не меняется", () => {
    const msg = "Ошибка базы данных: no such table: tasks";
    expect(localizeBackendError(msg, ru)).toBe(msg);
  });

  // The trap this function exists to avoid. Domain messages contain ": " as well,
  // so a naive split-on-first-colon would treat "Недопустимое расширение" as a
  // prefix, fail to find it in the dictionary and — worse — reassemble the message
  // through its own separator. Only the four known heads may be touched.
  it("доменное сообщение с двоеточием не трогается", () => {
    expect(localizeBackendError("Недопустимое расширение: png", en))
      .toBe("Недопустимое расширение: png");
    expect(localizeBackendError("Некорректный base64: unexpected end", en))
      .toBe("Некорректный base64: unexpected end");
    expect(localizeBackendError("Задача не найдена: abc", en))
      .toBe("Задача не найдена: abc");
  });

  // The case that actually separates this implementation from a naive
  // split-on-first-colon. When a domain message's head happens to be a dictionary
  // key, the naive version translates it: "Помодоро: 4 из 5" becomes
  // "Pomodoro: 4 из 5" — a half-translated string nobody asked for. The dictionary
  // holds 300+ everyday words, and the backend produces at least five such heads
  // ("Активное время", "В работе", "Далее", "Помодоро", "Уведомления"), so this is
  // not hypothetical.
  it("голова доменного сообщения не переводится, даже если она есть в словаре", () => {
    expect(localizeBackendError("Помодоро: 4 из 5", en)).toBe("Помодоро: 4 из 5");
    expect(localizeBackendError("Уведомления: 3 новых", en)).toBe("Уведомления: 3 новых");
    expect(localizeBackendError("В работе: две задачи", en)).toBe("В работе: две задачи");
    // the premise itself: these keys really are in the dictionary
    expect(en("Помодоро")).not.toBe("Помодоро");
    expect(en("Уведомления")).not.toBe("Уведомления");
  });

  it("строка без двоеточия возвращается как есть", () => {
    expect(localizeBackendError("Ревизия не найдена", en)).toBe("Ревизия не найдена");
    expect(localizeBackendError("", en)).toBe("");
  });

  // "Ошибка базы данных" without the ": " is not a prefixed message — it is the
  // whole text, and slicing it would produce an empty tail.
  it("голый префикс без деталей не расчленяется", () => {
    expect(localizeBackendError("Ошибка базы данных", en)).toBe("Ошибка базы данных");
  });

  it("префикс распознаётся только в начале строки", () => {
    const msg = "Не удалось сохранить, причина — Ошибка базы данных: disk full";
    expect(localizeBackendError(msg, en)).toBe(msg);
  });
});

// Wrapping a render site is a manual step, and it has already been forgotten once:
// a grep looked for `Store.error)}` with the closing paren and missed the bare
// `{taskStore.error}` in Tasks.svelte, where the error rendered untranslated.
describe("места отрисовки ошибок обёрнуты", () => {
  const VIEWS = import.meta.glob("/src/**/*.svelte", {
    query: "?raw",
    import: "default",
    eager: true,
  }) as Record<string, string>;

  it("ни одно .error не попадает в разметку без tErr", () => {
    const offenders: string[] = [];
    for (const [path, src] of Object.entries(VIEWS)) {
      // {something.error} with no wrapper call around it
      for (const m of src.matchAll(/\{([A-Za-z.]*\.error)\}/g)) {
        offenders.push(`${path}: {${m[1]}}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
