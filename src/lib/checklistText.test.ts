import { describe, it, expect } from "vitest";
import {
  parseChecklist,
  formatChecklist,
  toggleLine,
  lineIndexAt,
  CHECK_RE,
} from "./checklistText";

describe("parseChecklist", () => {
  it("читает отметки из префиксов", () => {
    const r = parseChecklist("[x] купить билеты\n[ ] собрать сумку");
    expect(r).toEqual([
      { title: "купить билеты", done: true },
      { title: "собрать сумку", done: false },
    ]);
  });

  it("строка без префикса — невыполненная подзадача", () => {
    // Главный сценарий вставки: список скопирован откуда угодно, размечать
    // его руками пользователь не обязан.
    expect(parseChecklist("собрать сумку")).toEqual([
      { title: "собрать сумку", done: false },
    ]);
  });

  it("пустые строки не становятся подзадачами", () => {
    // Пользователь жмёт Enter раньше, чем начинает печатать — это норма, а не
    // повод создать подзадачу без названия.
    expect(parseChecklist("[x] раз\n\n\n[ ] два\n   \n")).toHaveLength(2);
  });

  it("понимает markdown-список и заглавную X", () => {
    const r = parseChecklist("- [X] раз\n  * [ ] два\n[]три");
    expect(r).toEqual([
      { title: "раз", done: true },
      { title: "два", done: false },
      { title: "три", done: false },
    ]);
  });

  it("текст в скобках внутри названия не путается с префиксом", () => {
    // Префикс — только в начале строки; скобки дальше по тексту это данные.
    const r = parseChecklist("[ ] позвонить [важно]");
    expect(r).toEqual([{ title: "позвонить [важно]", done: false }]);
  });
});

describe("formatChecklist", () => {
  it("пишет префикс и невыполненным тоже", () => {
    // Без `[ ]` отметить строку было бы нечем: пришлось бы печатать скобки
    // руками вместо того, чтобы поставить x между готовых.
    expect(formatChecklist([{ title: "раз", done: false }])).toBe("[ ] раз");
  });

  it("парсинг и сборка обратимы", () => {
    const text = "[x] раз\n[ ] два";
    expect(formatChecklist(parseChecklist(text))).toBe(text);
  });
});

describe("toggleLine", () => {
  it("переключает нужную строку, не трогая соседние", () => {
    expect(toggleLine("[ ] раз\n[ ] два", 1)).toBe("[ ] раз\n[x] два");
    expect(toggleLine("[x] раз\n[ ] два", 0)).toBe("[ ] раз\n[ ] два");
  });

  it("нумерация пропускает пустые строки, как и разбор", () => {
    // Индекс приходит от отрисованного списка, где пустых строк нет; если бы
    // toggleLine считал их, галочка ставилась бы не на ту строку.
    expect(toggleLine("[ ] раз\n\n[ ] два", 1)).toBe("[ ] раз\n\n[x] два");
  });

  it("строке без префикса префикс дописывается", () => {
    expect(toggleLine("раз", 0)).toBe("[x] раз");
  });

  it("сохраняет пустые строки и текст без разметки", () => {
    // Пересборка из parseChecklist потеряла бы пустую строку — поэтому
    // toggleLine правит текст на месте, а не собирает его заново.
    expect(toggleLine("[ ] раз\n\n[ ] два", 0)).toBe("[x] раз\n\n[ ] два");
  });
});

// Границы CHECK_RE — контракт с редактором: на них считается диапазон, который
// прячется виджетом-чекбоксом. Съедет граница — пользователь увидит скобки или,
// наоборот, потеряет первую букву подзадачи.
describe("CHECK_RE (границы разметки для виджета)", () => {
  it("совпадение покрывает скобки и пробел после них, но не текст", () => {
    const m = CHECK_RE.exec("[x] купить билеты");
    expect(m?.[0]).toBe("[x] ");
    expect(m?.[1]).toBe("");
    expect(m?.[2]).toBe("x");
  });

  it("ведущий маркер списка уходит в первую группу", () => {
    // Группа 1 не прячется — иначе исчез бы отступ вложенного пункта.
    const m = CHECK_RE.exec("  - [ ] собрать сумку");
    expect(m?.[1]).toBe("  - ");
    expect(m?.[0]).toBe("  - [ ] ");
  });

  it("строка без разметки не совпадает", () => {
    expect(CHECK_RE.test("просто текст")).toBe(false);
  });

  it("текст в скобках внутри строки не совпадает", () => {
    expect(CHECK_RE.test("позвонить [важно]")).toBe(false);
  });
});

describe("lineIndexAt", () => {
  it("определяет строку под кареткой", () => {
    const text = "[ ] раз\n[ ] два\n[ ] три";
    expect(lineIndexAt(text, 0)).toBe(0);
    expect(lineIndexAt(text, text.indexOf("два") + 1)).toBe(1);
    expect(lineIndexAt(text, text.length)).toBe(2);
  });

  it("каретка в начале новой пустой строки не относится к предыдущей", () => {
    const text = "[ ] раз\n";
    expect(lineIndexAt(text, text.length)).toBe(1);
  });
});
