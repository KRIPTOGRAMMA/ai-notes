import { describe, it, expect } from "vitest";

// Checked against the source rather than the behaviour: the window buttons' logic
// lives in .svelte while vitest here is configured for pure .ts only. The same
// approach as in i18n.test.ts.
const SOURCES = import.meta.glob("./components/WindowControls.svelte", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

const SRC = SOURCES["./components/WindowControls.svelte"];

// The window config: without decorations: false the system title bar returns and all
// these buttons become a second set on top of the first.
const CONF = import.meta.glob("../../src-tauri/tauri.conf.json", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

describe("кнопки окна", () => {
  it("исходник компонента найден — путь не устарел", () => {
    expect(SRC).toBeTypeOf("string");
  });

  // The central invariant. The app lives in the tray: tracking, pomodoro and
  // notifications run in background loops while the window is hidden. Replacing
  // hide() with close() would kill them all and leave the tray behind — which would
  // look like "the application turns itself off".
  it("кнопка закрытия прячет окно, а не завершает процесс", () => {
    expect(SRC).toContain("hide()");
    // close() on the window is acceptable only as a handler's name, never as a call
    const callsWindowClose = /getCurrentWindow\(\)\s*\.\s*close\s*\(/.test(SRC);
    expect(callsWindowClose, "закрытие окна убьёт фоновые циклы").toBe(false);
  });

  // Without decorations: false on the main window WebKitGTK draws its own title bar
  // and our buttons end up as a second row beneath the system one.
  it("главное окно объявлено без системных декораций", () => {
    const raw = CONF["../../src-tauri/tauri.conf.json"];
    const conf = JSON.parse(raw);
    const main = conf.app.windows.find((w: { label: string }) => w.label === "main");
    expect(main, "окно main не найдено в конфиге").toBeTruthy();
    expect(main.decorations).toBe(false);
  });

  // A window without decorations has nothing to drag it by: the WM no longer provides
  // a title bar, so dragging must be initiated from the application.
  it("есть зона перетаскивания окна", () => {
    expect(SRC).toContain("startDragging()");
    expect(SRC).toMatch(/onmousedown/);
  });

  // The maximize/restore icon must follow the window's real state: maximizing is
  // possible by double-clicking and through the WM, bypassing these buttons.
  it("состояние «развёрнуто» синхронизируется с окном, а не только с кликами", () => {
    expect(SRC).toContain("isMaximized()");
    expect(SRC).toContain("onResized(");
  });
});
