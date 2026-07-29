import { describe, it, expect } from "vitest";

// Проверка по исходнику, а не по поведению: логика кнопок окна живёт в
// .svelte, а vitest здесь настроен только на чистые .ts. Тот же приём, что
// в i18n.test.ts.
const SOURCES = import.meta.glob("./components/WindowControls.svelte", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

const SRC = SOURCES["./components/WindowControls.svelte"];

// Конфиг окна: без decorations: false системный заголовок вернётся, и все
// эти кнопки станут вторым комплектом поверх первого.
const CONF = import.meta.glob("../../src-tauri/tauri.conf.json", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

describe("кнопки окна", () => {
  it("исходник компонента найден — путь не устарел", () => {
    expect(SRC).toBeTypeOf("string");
  });

  // Главный инвариант версии. Приложение живёт в трее: трекинг, помодоро и
  // уведомления крутятся в фоновых циклах, когда окно скрыто. Замена hide()
  // на close() убила бы их все, оставив трей, — и это выглядело бы как
  // «приложение само выключается».
  it("кнопка закрытия прячет окно, а не завершает процесс", () => {
    expect(SRC).toContain("hide()");
    // close() у окна допустим только как имя обработчика, но не как вызов
    const callsWindowClose = /getCurrentWindow\(\)\s*\.\s*close\s*\(/.test(SRC);
    expect(callsWindowClose, "закрытие окна убьёт фоновые циклы").toBe(false);
  });

  // Без decorations: false у главного окна WebKitGTK рисует свой заголовок,
  // и свои кнопки оказываются вторым рядом под системным.
  it("главное окно объявлено без системных декораций", () => {
    const raw = CONF["../../src-tauri/tauri.conf.json"];
    const conf = JSON.parse(raw);
    const main = conf.app.windows.find((w: { label: string }) => w.label === "main");
    expect(main, "окно main не найдено в конфиге").toBeTruthy();
    expect(main.decorations).toBe(false);
  });

  // Окно без декораций нечем таскать: WM больше не даёт шапку, перетаскивание
  // обязано инициироваться из приложения.
  it("есть зона перетаскивания окна", () => {
    expect(SRC).toContain("startDragging()");
    expect(SRC).toMatch(/onmousedown/);
  });

  // Иконка «развернуть/восстановить» должна следовать за реальным состоянием
  // окна: развернуть можно двойным кликом и средствами WM, мимо этих кнопок.
  it("состояние «развёрнуто» синхронизируется с окном, а не только с кликами", () => {
    expect(SRC).toContain("isMaximized()");
    expect(SRC).toContain("onResized(");
  });
});
