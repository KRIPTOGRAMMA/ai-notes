import { describe, it, expect } from "vitest";
import {
  parseChecklist,
  formatChecklist,
  toggleLine,
  lineIndexAt,
  removeLineAt,
  emptyAfterBackspace,
  dropEmptyLines,
  repairChecklistMarkup,
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

// v0.9.48: разметка `[ ] ` спрятана виджетом, поэтому «стереть подзадачу» для
// пользователя = стереть видимый текст. Раньше после этого оставалась строка
// `[ ] ` — на экране пустая строка с чекбоксом, в данных ничего.
describe("removeLineAt", () => {
  it("удаляет строку целиком вместе со скрытой разметкой", () => {
    const text = "[ ] раз\n[x] два\n[ ] три";
    expect(removeLineAt(text, text.indexOf("два"))).toBe("[ ] раз\n[ ] три");
  });

  it("на месте удалённой строки не остаётся пустой", () => {
    const text = "[ ] раз\n[ ] два";
    const out = removeLineAt(text, text.indexOf("два"));
    expect(out).toBe("[ ] раз");
    expect(out.split("\n")).toHaveLength(1);
  });

  // Первая строка — особый случай: перед ней нет перевода строки, и удалять
  // надо тот, что после неё, иначе вторая строка останется с пустой над ней.
  it("удаление первой строки поднимает вторую наверх", () => {
    const text = "[ ] раз\n[ ] два\n[ ] три";
    expect(removeLineAt(text, 0)).toBe("[ ] два\n[ ] три");
  });

  it("удаление единственной строки очищает поле", () => {
    expect(removeLineAt("[ ] одна", 3)).toBe("");
  });

  it("удаление последней строки не оставляет висящего перевода", () => {
    const text = "[ ] раз\n[ ] два";
    expect(removeLineAt(text, text.length)).toBe("[ ] раз");
  });

  // Удаление строки — операция над текстом, а не над списком подзадач:
  // пустые строки, которые пользователь набрал, но не заполнил, сохраняются.
  it("не трогает соседние строки и их отметки", () => {
    const text = "[x] сделано\n[ ] лишняя\n[x] тоже сделано";
    expect(removeLineAt(text, text.indexOf("лишняя")))
      .toBe("[x] сделано\n[x] тоже сделано");
  });

  it("удалённая строка исчезает из разбора", () => {
    const text = "[ ] раз\n[ ] два";
    const after = removeLineAt(text, text.indexOf("два"));
    expect(parseChecklist(after).map(i => i.title)).toEqual(["раз"]);
  });
});


// Главный сценарий удаления: пользователь стирает подзадачу С КОНЦА, а не
// ставит каретку в начало строки. Когда исчезает последняя буква, подзадача
// должна исчезнуть вместе с ней — иначе на экране остаётся пустая строка с
// чекбоксом и требуется ещё одно нажатие по невидимым скобкам.
describe("emptyAfterBackspace", () => {
  it("удаление последней буквы опустошает строку", () => {
    // «[ ] я» — каретка в конце (col 5), стираем «я»
    expect(emptyAfterBackspace("[ ] я", 5)).toBe(true);
  });

  it("пока текст остаётся — это обычное укорачивание", () => {
    expect(emptyAfterBackspace("[ ] хлеб", 8)).toBe(false);
    expect(emptyAfterBackspace("[ ] хлеб", 6)).toBe(false);
  });

  it("разметка за текст не считается", () => {
    // Каретка сразу после скрытых скобок: текста нет вообще.
    expect(emptyAfterBackspace("[ ] ", 4)).toBe(true);
    expect(emptyAfterBackspace("[x] ", 4)).toBe(true);
  });

  it("каретка в начале строки с текстом — строка не пустеет", () => {
    // Backspace здесь склеил бы строки, но текст подзадачи никуда не делся.
    expect(emptyAfterBackspace("[ ] хлеб", 0)).toBe(false);
  });

  it("пробелы текстом не считаются", () => {
    expect(emptyAfterBackspace("[ ]   ", 6)).toBe(true);
  });

  it("работает на строке без разметки", () => {
    expect(emptyAfterBackspace("я", 1)).toBe(true);
    expect(emptyAfterBackspace("да", 2)).toBe(false);
  });

  it("удаление в середине слова строку не опустошает", () => {
    expect(emptyAfterBackspace("[ ] хлеб", 6)).toBe(false);
  });

  // Отступ и markdown-маркер — тоже разметка: строка `  - [ ] я` пустеет на
  // удалении единственной буквы, а не считается непустой из-за дефиса.
  it("markdown-маркер и отступ за текст не считаются", () => {
    expect(emptyAfterBackspace("  - [ ] я", 9)).toBe(true);
    expect(emptyAfterBackspace("  - [ ] яд", 10)).toBe(false);
  });
});

// v0.9.49: пустая подзадача (`[ ] ` после Enter) и голая пустая строка
// (Shift+Enter) видны на экране, но parseChecklist их выбрасывает — в БД они
// не попадают. Расхождение между тем, что видно, и тем, что сохранено.
describe("dropEmptyLines", () => {
  it("убирает пустую подзадачу с чекбоксом", () => {
    expect(dropEmptyLines("[ ] раз\n[ ] \n[ ] два")).toBe("[ ] раз\n[ ] два");
  });

  it("убирает голую пустую строку", () => {
    expect(dropEmptyLines("[ ] раз\n\n[ ] два")).toBe("[ ] раз\n[ ] два");
  });

  it("убирает строку из одних пробелов", () => {
    expect(dropEmptyLines("[ ] раз\n   \n[ ] два")).toBe("[ ] раз\n[ ] два");
  });

  it("сохраняет отметки и порядок", () => {
    expect(dropEmptyLines("[x] раз\n\n[ ] два\n[x] три"))
      .toBe("[x] раз\n[ ] два\n[x] три");
  });

  it("непустой список не меняется", () => {
    const text = "[x] раз\n[ ] два";
    expect(dropEmptyLines(text)).toBe(text);
  });

  it("список из одних пустых строк схлопывается в пустоту", () => {
    expect(dropEmptyLines("[ ] \n\n[ ] ")).toBe("");
    expect(dropEmptyLines("")).toBe("");
  });

  // Побочный эффект пересборки через formatChecklist, принятый сознательно:
  // разметка приводится к одному виду. Пользователь её не видит (она спрятана
  // виджетом), а разнобой пришёл бы только вставкой из буфера.
  it("нормализует разметку вставленного markdown", () => {
    expect(dropEmptyLines("- [X] раз\n  * [ ] два")).toBe("[x] раз\n[ ] два");
  });

  // Строка без разметки — подзадача (так работает вставка списка), поэтому
  // очистка её не выбрасывает, а дописывает префикс.
  it("строка без разметки становится подзадачей, а не мусором", () => {
    expect(dropEmptyLines("раз\n\nдва")).toBe("[ ] раз\n[ ] два");
  });
});

// v0.9.50: свой Backspace закрывал одну клавишу, а Ctrl+Backspace (удалить
// слово) шёл мимо и выедал скобки изнутри — в строке оставался видимый
// огрызок «[ ». Показывать разметку пользователю нельзя, поэтому чинится
// результат любого удаления, а не перечень клавиш.
describe("repairChecklistMarkup", () => {
  it("целую разметку не трогает", () => {
    const text = "[ ] раз\n[x] два";
    expect(repairChecklistMarkup(text)).toBe(text);
  });

  it("восстанавливает префикс у строки с огрызком и текстом", () => {
    expect(repairChecklistMarkup("[ раз")).toBe("[ ] раз");
    expect(repairChecklistMarkup("] раз")).toBe("[ ] раз");
    expect(repairChecklistMarkup("[x раз")).toBe("[ ] раз");
  });

  // Огрызок должен быть отделён от текста пробелом. `[раз` не чинится
  // намеренно: отличить остаток разметки от слова, начатого со скобки,
  // невозможно, а испортить набранный текст хуже, чем оставить скобку.
  it("скобка, приклеенная к слову, — текст, а не огрызок", () => {
    expect(repairChecklistMarkup("[раз")).toBe("[раз");
    expect(repairChecklistMarkup("[важно] сделать")).toBe("[важно] сделать");
    expect(repairChecklistMarkup("[xyz] код")).toBe("[xyz] код");
    expect(repairChecklistMarkup("[TODO] дело")).toBe("[TODO] дело");
  });

  it("строка из одного огрызка становится пустой", () => {
    // Дальше её уберёт dropEmptyLines при уходе фокуса.
    expect(repairChecklistMarkup("[ ")).toBe("");
    expect(repairChecklistMarkup("[")).toBe("");
  });

  it("чинит только испорченные строки, соседние сохраняет", () => {
    expect(repairChecklistMarkup("[x] раз\n[ два\n[ ] три"))
      .toBe("[x] раз\n[ ] два\n[ ] три");
  });

  it("сохраняет отступ и markdown-маркер", () => {
    expect(repairChecklistMarkup("  - [ дело")).toBe("  - [ ] дело");
  });

  // Главный риск этой функции: съесть законный текст. Строка без разметки —
  // валидная подзадача (так работает вставка списка), трогать её нельзя.
  it("строку без разметки не трогает", () => {
    expect(repairChecklistMarkup("просто текст")).toBe("просто текст");
    expect(repairChecklistMarkup("раз\nдва")).toBe("раз\nдва");
  });

  it("скобки в середине текста — это данные, а не разметка", () => {
    expect(repairChecklistMarkup("[ ] позвонить [важно]"))
      .toBe("[ ] позвонить [важно]");
    expect(repairChecklistMarkup("позвонить [важно]"))
      .toBe("позвонить [важно]");
  });

  it("результат починки читается разбором как подзадача", () => {
    const fixed = repairChecklistMarkup("[ купить хлеб");
    expect(parseChecklist(fixed)).toEqual([{ title: "купить хлеб", done: false }]);
  });
});
