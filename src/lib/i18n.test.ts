import { describe, it, expect } from "vitest";
import { translate, detectLang, seededName, SEEDED_CATEGORY_IDS, LANGS } from "./i18n";
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

// v0.9.47: категории и статусы приходят из БД, поэтому ни один статический
// тест по исходникам их не видел — «Работа» и «В работе» оставались русскими
// на английском интерфейсе. Переводим только посевные записи и только пока их
// не тронул пользователь.
describe("seededName", () => {
  it("посевная категория переводится по id", () => {
    expect(seededName("category", "Work", "Работа", "en")).toBe("Work");
    expect(seededName("category", "Health", "Здоровье", "en")).toBe("Health");
  });

  it("посевной статус переводится по id", () => {
    expect(seededName("status", "InProgress", "В работе", "en")).toBe("In progress");
    expect(seededName("status", "Archived", "Архив", "en")).toBe("Archive");
  });

  it("на русском отдаёт исходное имя", () => {
    expect(seededName("category", "Work", "Работа", "ru")).toBe("Работа");
    expect(seededName("status", "Done", "Готово", "ru")).toBe("Готово");
  });

  // Главное свойство: имя, которое написал пользователь, — его текст.
  // Переводить его нельзя ни при каком совпадении с посевным.
  //
  // «Работа» здесь проверяет проверку по id в чистом виде: id не посевной,
  // поэтому сверка с оригиналом не участвует (uuid нет в таблице оригиналов),
  // а имя переводимое — значит, отработать может только отсечка по id.
  it("пользовательская категория не переводится", () => {
    expect(seededName("category", "b3f1c2a4-uuid", "Работа", "en")).toBe("Работа");
    expect(seededName("category", "b3f1c2a4-uuid", "Мои дела", "en")).toBe("Мои дела");
  });

  it("переименованная посевная категория не переводится", () => {
    // id посевной, но имя уже не то — значит, пользователь его сменил.
    // Перевод здесь спрятал бы правку от него самого.
    //
    // Новые имена взяты ИЗ СЛОВАРЯ намеренно: на строке, которой в словаре
    // нет, translate() и так вернул бы её как есть, и тест прошёл бы даже
    // без сверки с оригиналом — проверял бы неполноту словаря, а не защиту.
    expect(seededName("category", "Work", "Здоровье", "en")).toBe("Здоровье");
    expect(seededName("status", "Done", "Архив", "en")).toBe("Архив");
  });

  // kind — часть ключа, а не украшение: без него статус «Done» нашёлся бы
  // среди категорий и наоборот. Пары ниже не существуют ни в одной таблице.
  it("kind участвует в поиске оригинала", () => {
    expect(seededName("category", "Done", "Готово", "en")).toBe("Готово");
    expect(seededName("status", "Work", "Работа", "en")).toBe("Работа");
  });

  // Настройки блокируют поле переименования по этому списку. Разъедься он с
  // таблицей оригиналов — посевная категория стала бы редактируемой, и первая
  // же правка записала бы английский перевод в БД поверх русского оригинала.
  it("список id для Настроек совпадает с посевными категориями", () => {
    expect([...SEEDED_CATEGORY_IDS].sort())
      .toEqual(["Health", "Home", "Other", "Study", "Work"]);
  });

  it("все посевные имена есть в словаре EN", () => {
    for (const name of ["Работа", "Учёба", "Дом", "Здоровье", "Другое", "В работе", "Готово", "Архив"]) {
      expect(EN[name], `нет перевода для посевного имени «${name}»`).toBeTypeOf("string");
    }
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
    "/src/lib/components/ChecklistEditor.svelte",
  ];

  it("все объявленные файлы найдены — путь не устарел", () => {
    for (const f of LOCALIZED) {
      expect(SOURCES[f], `не найден файл ${f}`).toBeTypeOf("string");
    }
  });

  // v0.9.46: тест выше ловит только строки, УЖЕ обёрнутые в t(). Русская
  // строка, которую забыли обернуть, была для него невидима — именно так
  // «локализация завершена» (v0.9.38) разошлась с реальностью: пользователь
  // нашёл русский текст в Настройках, Sidebar, графе и подсказках. Этот тест
  // смотрит с другой стороны: в размеченных файлах не должно остаться
  // кириллицы вне t()/tr().
  //
  // Что сознательно НЕ считается нарушением:
  // - комментарии (весь код проекта комментируется по-русски);
  // - <style> (там кириллица бывает только в комментариях);
  // - ключи внутри самих t("...") — это и есть словарные ключи;
  // - блоки, помеченные `/* i18n-ok */` — их переводит не место объявления,
  //   а место отрисовки (списки NAV и команд палитры: `{t(item.label)}`).
  //   Пометка снимает проверку с блока до ближайшей строки `];` и обязана
  //   быть явной: иначе тест либо молчит о реальных пропусках, либо требует
  //   «чинить» рабочий код.
  it("в размеченных файлах нет кириллицы вне t()", () => {
    const offenders: string[] = [];
    for (const file of LOCALIZED) {
      let src = SOURCES[file] ?? "";
      src = src.replace(/<style[\s\S]*?<\/style>/g, "");
      src = src.replace(/<!--[\s\S]*?-->/g, "");
      // Порядок важен: пометку `/* i18n-ok */` защищаем ДО вырезания
      // блочных комментариев, иначе она удаляется вместе с ними и блок
      // снова считается нарушением.
      src = src.replace(/\/\*\s*i18n-ok\s*\*\//g, "@@I18N_OK@@");
      src = src.replace(/\/\*[\s\S]*?\*\//g, "");
      src = src.replace(/(^|[^:"'`\\])\/\/.*$/gm, (m, p1) =>
        m.includes("@@I18N_OK@@") ? `${p1}@@I18N_OK@@` : p1);
      src = src.replace(/@@I18N_OK@@/g, "i18n-ok");
      // Вырезаем содержимое t("...") / tr("...") — оно обязано быть русским.
      // Одинарные кавычки нужны наравне с двойными: внутри атрибута разметки
      // (`title="{t('Поиск')} (Ctrl+K)"`) иначе не написать, и без этой ветки
      // тест считал бы уже переведённую строку нарушением.
      src = src.replace(/(?<![\w.])tr?\((["'])(?:(?!\1).)*\1/g, "t()");
      let skipUntilClose = false;
      for (const [i, line] of src.split("\n").entries()) {
        if (line.includes("i18n-ok")) { skipUntilClose = true; continue; }
        if (skipUntilClose) {
          if (/^\s*\];/.test(line)) skipUntilClose = false;
          continue;
        }
        if (!/[а-яА-Я]/.test(line)) continue;
        offenders.push(`${file}:${i + 1}: ${line.trim()}`);
      }
    }
    expect(offenders, `не обёрнуто в t():\n${offenders.join("\n")}`).toEqual([]);
  });

  // Справка (help.ts) — чистые данные без t(): переводится при отрисовке в
  // Settings.svelte (`{t(item.desc)}`). Поэтому предыдущие два теста её не
  // видят вовсе, и без этой проверки новая тема справки молча осталась бы
  // русской на английском интерфейсе — ровно так и вышло в v0.9.29→v0.9.45.
  it("вся справка (help.ts) есть в словаре EN", () => {
    const HELP_SRC = import.meta.glob("/src/lib/help.ts", {
      query: "?raw", import: "default", eager: true,
    }) as Record<string, string>;
    const src = HELP_SRC["/src/lib/help.ts"] ?? "";
    expect(src, "help.ts не найден — путь устарел").not.toBe("");
    const missing: string[] = [];
    for (const m of src.matchAll(/(?:title|term|desc):\s*\n?\s*"((?:[^"\\]|\\.)*)"/g)) {
      const key = m[1].replace(/\\"/g, '"');
      if (!(key in EN)) missing.push(key);
    }
    expect(missing, `нет перевода:\n${missing.join("\n")}`).toEqual([]);
  });

  // keybinds.ts — та же схема, что help.ts: чистые данные, перевод при
  // отрисовке (`{t(action.label)}` в Settings). Названия действий видны на
  // вкладке «Хоткеи», и без этой проверки новое действие осталось бы русским.
  it("названия действий (keybinds.ts) есть в словаре EN", () => {
    const KB = import.meta.glob("/src/lib/keybinds.ts", {
      query: "?raw", import: "default", eager: true,
    }) as Record<string, string>;
    const src = KB["/src/lib/keybinds.ts"] ?? "";
    expect(src, "keybinds.ts не найден — путь устарел").not.toBe("");
    const missing: string[] = [];
    for (const m of src.matchAll(/label: "([^"]+)"/g)) {
      if (!(m[1] in EN)) missing.push(m[1]);
    }
    expect(missing, `нет перевода:\n${missing.join("\n")}`).toEqual([]);
  });

  // Описания моделей приходят из Rust (commands/model.rs) и переводятся при
  // отрисовке в ModelDownloader.svelte. Ни один из тестов выше их не видит:
  // сам .svelte-файл размечен и чист, а кириллица живёт за пределами /src —
  // ровно тот же слепой участок, из-за которого вкладка ИИ осталась русской
  // после «локализация завершена» (v0.9.46).
  it("описания моделей (model.rs) есть в словаре EN", () => {
    const RS = import.meta.glob("/src-tauri/src/commands/model.rs", {
      query: "?raw", import: "default", eager: true,
    }) as Record<string, string>;
    const src = RS["/src-tauri/src/commands/model.rs"] ?? "";
    expect(src, "model.rs не найден — путь устарел").not.toBe("");
    const missing: string[] = [];
    let found = 0;
    for (const m of src.matchAll(/description:\s*"((?:[^"\\]|\\.)*)"/g)) {
      found++;
      const key = m[1].replace(/\\"/g, '"');
      if (!(key in EN)) missing.push(key);
    }
    // Иначе тест «проходит», перестав что-либо находить: сменится формат
    // строки в model.rs — и пустой список молча сойдёт за отсутствие пропусков.
    expect(found, "в model.rs не найдено ни одного описания — изменился формат").toBeGreaterThan(0);
    expect(missing, `нет перевода:\n${missing.join("\n")}`).toEqual([]);
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
