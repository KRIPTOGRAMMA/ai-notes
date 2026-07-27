import { describe, it, expect } from "vitest";
import { describeError, runGuarded } from "./guard";

describe("describeError", () => {
  // Rust-команды через invoke отдают именно строку, не Error — это основной
  // путь в приложении, и раньше такие ошибки нигде не показывались.
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

  // Ключевое поведение v0.9.25: успех должен различаться от ошибки, чтобы
  // стор мог сбросить error. Раньше error только выставлялся и первая же
  // неудача оставляла баннер висеть навсегда.
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
