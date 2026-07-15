import { describe, it, expect, beforeEach, vi } from "vitest";
import { applyTheme, applyCachedTheme } from "./theme";

// jsdom не реализует matchMedia, а localStorage jsdom перекрывается «пустым»
// экспериментальным глобалом Node — стабим оба управляемыми заглушками.
let systemDark = false;
const listeners = new Set<(e: MediaQueryListEvent) => void>();
const store = new Map<string, string>();

vi.stubGlobal("localStorage", {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, String(v)),
  removeItem: (k: string) => void store.delete(k),
  clear: () => store.clear(),
});

beforeEach(() => {
  systemDark = false;
  listeners.clear();
  localStorage.clear();
  document.documentElement.classList.remove("dark");
  document.documentElement.removeAttribute("style");
  vi.stubGlobal("matchMedia", (query: string) => ({
    get matches() { return systemDark; },
    media: query,
    addEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => listeners.delete(cb),
  }));
});

function fireSystemThemeChange(dark: boolean) {
  systemDark = dark;
  for (const cb of [...listeners]) cb({ matches: dark } as MediaQueryListEvent);
}

describe("applyTheme", () => {
  it("dark ставит класс, light снимает", () => {
    applyTheme("dark", {});
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    applyTheme("light", {});
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("system следует за системной темой и реагирует на её смену", () => {
    systemDark = true;
    applyTheme("system", {});
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    fireSystemThemeChange(false);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("не копит слушателей и отписывается при уходе с system", () => {
    applyTheme("system", {});
    applyTheme("system", {});
    expect(listeners.size).toBe(1);

    applyTheme("light", {});
    expect(listeners.size).toBe(0);
    // смена системной темы больше не влияет
    fireSystemThemeChange(true);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("кастомный акцент выставляет --accent и осветлённый --accent-hover", () => {
    applyTheme("light", { color_accent: "#000000" });
    const root = document.documentElement;
    expect(root.style.getPropertyValue("--accent")).toBe("#000000");
    // 0 + 255*0.15 ≈ 38 → #262626
    expect(root.style.getPropertyValue("--accent-hover")).toBe("#262626");
  });

  it("пустой цвет убирает переопределение (возврат к CSS-дефолту)", () => {
    applyTheme("light", { color_accent: "#112233" });
    applyTheme("light", { color_accent: "" });
    expect(document.documentElement.style.getPropertyValue("--accent")).toBe("");
    expect(document.documentElement.style.getPropertyValue("--accent-hover")).toBe("");
  });
});

describe("applyCachedTheme", () => {
  it("восстанавливает режим и цвета из localStorage", () => {
    applyTheme("dark", { color_accent: "#ff0000" });
    document.documentElement.classList.remove("dark");
    document.documentElement.removeAttribute("style");

    applyCachedTheme();
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--accent")).toBe("#ff0000");
  });

  it("битый кеш падает обратно на system без исключения", () => {
    localStorage.setItem("theme_colors", "не json");
    expect(() => applyCachedTheme()).not.toThrow();
  });
});
