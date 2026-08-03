import { describe, it, expect } from "vitest";
import { describeError, runGuarded } from "./guard";

describe("describeError", () => {
  // Rust commands invoked through invoke return a string rather than an Error. That
  // is the main path in the application, and such errors used to be shown nowhere.
  it("строка от Rust-команды проходит как есть", () => {
    expect(describeError("Задача не найдена: abc")).toBe("Задача не найдена: abc");
  });

  it("Error разворачивается в message", () => {
    expect(describeError(new Error("сеть недоступна"))).toBe("сеть недоступна");
  });

  it("всё остальное — понятный фолбэк вместо [object Object]", () => {
    expect(describeError({ code: 500 })).toBe("Неизвестная ошибка");
    expect(describeError(null)).toBe("Неизвестная ошибка");
    expect(describeError(undefined)).toBe("Неизвестная ошибка");
  });
});

describe("runGuarded", () => {
  it("успех отдаёт значение", async () => {
    const r = await runGuarded(async () => 42);
    expect(r).toEqual({ ok: true, value: 42 });
  });

  it("падение превращается в описанную ошибку, а не пробрасывается", async () => {
    const r = await runGuarded(async () => { throw "Пустая подзадача"; });
    expect(r).toEqual({ ok: false, error: "Пустая подзадача" });
  });

  // The key behaviour: success must be distinguishable from failure so the store can
  // clear error. It used to be only set, and the very first failure left the banner
  // hanging forever.
  it("успех после ошибки различим — стору есть по чему сбросить error", async () => {
    const fail = await runGuarded(async () => { throw "упало"; });
    const ok = await runGuarded(async () => "ок");
    expect(fail.ok).toBe(false);
    expect(ok.ok).toBe(true);
  });

  it("falsy-значения не путаются с ошибкой", async () => {
    expect(await runGuarded(async () => null)).toEqual({ ok: true, value: null });
    expect(await runGuarded(async () => 0)).toEqual({ ok: true, value: 0 });
    expect(await runGuarded(async () => false)).toEqual({ ok: true, value: false });
  });
});

// Six stores each carried a private copy of describeError, and five of those never
// cleared error on success — so the first failure pinned the banner until a reload.
// The sources are scanned the same way i18n.test.ts scans for untranslated strings.
describe("сторы пользуются общим guard", () => {
  const STORES = import.meta.glob("/src/lib/stores/*.svelte.ts", {
    query: "?raw",
    import: "default",
    eager: true,
  }) as Record<string, string>;

  it("ни один стор не заводит свой describeError", () => {
    const offenders = Object.entries(STORES)
      .filter(([, src]) => /function\s+describeError/.test(src))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  it("каждый стор с error импортирует runGuarded", () => {
    const offenders = Object.entries(STORES)
      .filter(([, src]) => /let error: string \| null = \$state/.test(src))
      .filter(([, src]) => !/from "\.\.\/guard"/.test(src))
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });

  // The actual bug: error was only ever assigned. Every store that owns an error
  // rune must also clear it somewhere other than clearError().
  it("каждый стор сбрасывает error при успехе", () => {
    const offenders = Object.entries(STORES)
      .filter(([, src]) => /let error: string \| null = \$state/.test(src))
      .filter(([, src]) => {
        const clears = src.match(/error = null/g)?.length ?? 0;
        return clears < 2; // one of them is always clearError()
      })
      .map(([path]) => path);
    expect(offenders).toEqual([]);
  });
});
