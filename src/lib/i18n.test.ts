import { describe, it, expect } from "vitest";
import { translate, detectLang, LANGS } from "./i18n";
import { EN } from "./i18n.en";

describe("translate", () => {
  it("русский возвращает ключ как есть — он и есть оригинал", () => {
    expect(translate("Задачи", "ru")).toBe("Задачи");
    // даже если ключа нет в словаре EN
    expect(translate("Никогда не переводившаяся строка", "ru"))
      .toBe("Никогда не переводившаяся строка");
  });

  it("английский берёт перевод из словаря", () => {
    expect(translate("Задачи", "en")).toBe("Tasks");
    expect(translate("Заметки", "en")).toBe("Notes");
  });

  // Ключевое свойство схемы «ключ = русский текст»: недоделанный перевод
  // деградирует в читаемую русскую строку, а не в «tasks.empty_state»
  // или пустоту на экране.
  it("отсутствующий перевод отдаёт русский оригинал, а не пустоту", () => {
    const missing = "Строка, которой точно нет в словаре 12345";
    expect(translate(missing, "en")).toBe(missing);
    expect(translate(missing, "en")).not.toBe("");
  });

  it("подстановка переменных работает в обоих языках", () => {
    expect(translate("Очищено записей: {n}", "ru", { n: 3 })).toBe("Очищено записей: 3");
    expect(translate("Очищено записей: {n}", "en", { n: 3 })).toBe("Rows cleared: 3");
  });

  it("повторяющаяся переменная подставляется везде", () => {
    expect(translate("{a} и ещё {a}", "ru", { a: "раз" })).toBe("раз и ещё раз");
  });

  it("лишние переменные не ломают строку, отсутствующие остаются как есть", () => {
    expect(translate("Просто текст", "ru", { unused: 1 })).toBe("Просто текст");
    expect(translate("Значение: {x}", "ru")).toBe("Значение: {x}");
  });
});

describe("detectLang", () => {
  it("русская локаль — русский", () => {
    expect(detectLang("ru")).toBe("ru");
    expect(detectLang("ru-RU")).toBe("ru");
    expect(detectLang("RU-ru")).toBe("ru");
  });

  // Всё нерусское — английский: нероссийскому пользователю английский
  // полезнее русского, обратное неверно.
  it("любая другая локаль — английский", () => {
    expect(detectLang("en-US")).toBe("en");
    expect(detectLang("de")).toBe("en");
    expect(detectLang("")).toBe("en");
  });
});

describe("словарь EN", () => {
  it("в нём нет пустых переводов", () => {
    for (const [key, val] of Object.entries(EN)) {
      expect(val.trim(), `пустой перевод для «${key}»`).not.toBe("");
    }
  });

  // Перевод, совпадающий с оригиналом, обычно означает забытую строку,
  // а не намеренное решение. Исключения — общие для обоих языков слова.
  it("переводы не совпадают с русским оригиналом", () => {
    const sameAsKey = Object.entries(EN).filter(([k, v]) => k === v);
    expect(sameAsKey).toEqual([]);
  });

  it("плейсхолдеры перевода совпадают с оригиналом", () => {
    const re = /\{(\w+)\}/g;
    for (const [key, val] of Object.entries(EN)) {
      const inKey = [...key.matchAll(re)].map(m => m[1]).sort();
      const inVal = [...val.matchAll(re)].map(m => m[1]).sort();
      expect(inVal, `плейсхолдеры разошлись в «${key}»`).toEqual(inKey);
    }
  });

  it("оба языка объявлены в LANGS", () => {
    expect(LANGS.map(l => l.id).sort()).toEqual(["en", "ru"]);
  });
});

// v0.9.36: главный риск постепенного наполнения словаря — обернуть строку в
// t() и забыть добавить перевод. Тогда английский интерфейс молча покажет
// русскую строку: механизм так и задуман (деградация лучше пустоты), но для
// УЖЕ размеченных файлов это баг, а не незаконченный перевод.
//
// Поэтому проверяется не «весь UI переведён», а более узкое и честное: в
// файлах, которые мы объявили переведёнными, каждый ключ есть в словаре.
// Список файлов растёт по мере локализации — новый файл добавляется сюда же.
describe("покрытие словаря по размеченным файлам", () => {
  // Файлы читаются через import.meta.glob (Vite, `as: "raw"`), а не через
  // node:fs: @types/node в проекте нет, и тянуть его ради одного теста
  // несоразмерно — glob типизирован самим Vite и работает в jsdom-окружении.
  const SOURCES = import.meta.glob("/src/**/*.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

  // Список растёт по мере локализации: сюда добавляется файл, который мы
  // объявили переведённым целиком.
  const LOCALIZED = [
    "/src/App.svelte",
    "/src/views/Settings.svelte",
    "/src/views/Tasks.svelte",
    "/src/views/Notes.svelte",
    "/src/views/Calendar.svelte",
    "/src/views/Dashboard.svelte",
    "/src/lib/components/TaskModal.svelte",
    "/src/lib/components/QuickCapture.svelte",
    "/src/views/Today.svelte",
    "/src/views/Onboarding.svelte",
    "/src/lib/components/LiveMarkdownEditor.svelte",
    "/src/lib/components/RoutinesModal.svelte",
    "/src/lib/components/PomodoroWidget.svelte",
    "/src/lib/components/SearchOverlay.svelte",
    "/src/lib/components/NotificationPanel.svelte",
    "/src/lib/components/TrackingWidget.svelte",
    "/src/lib/components/ModelDownloader.svelte",
    "/src/views/NotesGraph.svelte",
    "/src/lib/components/TaskHistoryDetail.svelte",
    "/src/lib/components/WindowControls.svelte",
  ];

  it("все объявленные файлы найдены — путь не устарел", () => {
    for (const f of LOCALIZED) {
      expect(SOURCES[f], `не найден файл ${f}`).toBeTypeOf("string");
    }
  });

  it("каждый t(\"...\") из размеченных файлов есть в словаре EN", () => {
    const missing: string[] = [];
    for (const file of LOCALIZED) {
      const src = SOURCES[file] ?? "";
      // `tr(` — Calendar.svelte: там `t` занято переменной задачи в {#each},
      // и перевод импортирован как `tr`. Без этой ветки тест молча считал бы
      // файл переведённым, не проверив ни одного его ключа.
      // (?<![\w.]) — чтобы не поймать split("...") / import("...")
      for (const m of src.matchAll(/(?<![\w.])tr?\("([^"]+)"/g)) {
        if (!(m[1] in EN)) missing.push(`${file}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
