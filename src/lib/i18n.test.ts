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
