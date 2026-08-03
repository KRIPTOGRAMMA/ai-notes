import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import {
  insertAtCursor, wrapSelection, toggleLinePrefix, toggleOrderedList, insertLink, insertTable,
} from "./commands";

// CodeMirror needs a DOM; vitest runs jsdom (vitest.config.ts), and the view
// mounts there fine. Until v0.9.72 these functions closed over the component's
// `view`, so none of this could be covered by unit tests.
function editor(doc: string, anchor?: number, head?: number): EditorView {
  const view = new EditorView({ state: EditorState.create({ doc }), parent: document.body });
  if (anchor !== undefined) {
    view.dispatch({ selection: { anchor, head: head ?? anchor } });
  }
  return view;
}

const textOf = (v: EditorView) => v.state.doc.toString();

describe("insertAtCursor", () => {
  // The padding rules exist because whisper returns a bare phrase: dictating with
  // the caret mid-sentence would otherwise glue it to the neighbouring word.
  it("каретка в середине слова — пробелы с обеих сторон", () => {
    const v = editor("началоконец", 6);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("начало вставка конец");
    v.destroy();
  });

  // Каретка стоит за пробелом, но вплотную к слову справа: слева отступ не нужен,
  // справа нужен. Стороны считаются независимо — в v0.9.65 padding был сделан
  // только слева, и получалось «начало вставкаконец».
  it("после пробела слева не добавляется, справа добавляется", () => {
    const v = editor("начало конец", 7);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("начало вставка конец");
    v.destroy();
  });

  it("в начале документа слева не добавляется", () => {
    const v = editor("конец", 0);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("вставка конец");
    v.destroy();
  });

  it("в конце документа справа не добавляется", () => {
    const v = editor("начало", 6);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("начало вставка");
    v.destroy();
  });

  // Exactly the case the v0.9.66 test got wrong: after a newline charBefore is
  // "\n", which is whitespace, so no padding is correct.
  it("перед переводом строки справа не добавляется", () => {
    const v = editor("начало\nконец", 6);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("начало вставка\nконец");
    v.destroy();
  });

  it("после перевода строки слева не добавляется", () => {
    const v = editor("начало\n", 7);
    insertAtCursor(v, "вставка");
    expect(textOf(v)).toBe("начало\nвставка");
    v.destroy();
  });

  it("непустое выделение заменяется, отступы считаются по соседям", () => {
    const v = editor("началоСТАРОЕконец", 6, 12);
    insertAtCursor(v, "новое");
    expect(textOf(v)).toBe("начало новое конец");
    v.destroy();
  });

  it("пустой текст ничего не меняет", () => {
    const v = editor("начало", 3);
    insertAtCursor(v, "");
    expect(textOf(v)).toBe("начало");
    v.destroy();
  });

  it("каретка встаёт после вставки, а не в начале", () => {
    const v = editor("началоконец", 6);
    insertAtCursor(v, "вставка");
    // "начало вставка конец" — каретка сразу за вставленным куском с отступами
    expect(v.state.selection.main.head).toBe(6 + " вставка ".length);
    v.destroy();
  });
});

describe("wrapSelection", () => {
  it("оборачивает выделение", () => {
    const v = editor("жирный текст", 0, 6);
    wrapSelection(v, "**", "**");
    expect(textOf(v)).toBe("**жирный** текст");
    v.destroy();
  });

  // Without this a second Ctrl+B would keep piling ** on the outside.
  it("повторное нажатие снимает обёртку", () => {
    const v = editor("**жирный** текст", 0, 10);
    wrapSelection(v, "**", "**");
    expect(textOf(v)).toBe("жирный текст");
    v.destroy();
  });

  it("без выделения вставляет пустую пару, каретка внутри", () => {
    const v = editor("текст", 5);
    wrapSelection(v, "**", "**");
    expect(textOf(v)).toBe("текст****");
    expect(v.state.selection.main.head).toBe(7);
    v.destroy();
  });
});

describe("toggleLinePrefix", () => {
  it("добавляет префикс строке", () => {
    const v = editor("заголовок", 3);
    toggleLinePrefix(v, "## ");
    expect(textOf(v)).toBe("## заголовок");
    v.destroy();
  });

  it("повторный вызов снимает префикс", () => {
    const v = editor("## заголовок", 5);
    toggleLinePrefix(v, "## ");
    expect(textOf(v)).toBe("заголовок");
    v.destroy();
  });
});

describe("toggleOrderedList", () => {
  it("нумерует выделенные строки с единицы", () => {
    const v = editor("первый\nвторой\nтретий", 0, 20);
    toggleOrderedList(v);
    expect(textOf(v)).toBe("1. первый\n2. второй\n3. третий");
    v.destroy();
  });

  it("пустые строки не нумеруются", () => {
    const v = editor("первый\n\nвторой", 0, 14);
    toggleOrderedList(v);
    expect(textOf(v)).toBe("1. первый\n\n2. второй");
    v.destroy();
  });

  it("снимает нумерацию, если она есть у всех строк", () => {
    const v = editor("1. первый\n2. второй", 0, 19);
    toggleOrderedList(v);
    expect(textOf(v)).toBe("первый\nвторой");
    v.destroy();
  });

  // A click on a partially numbered block must not silently drop the numbers.
  it("частично пронумерованный блок перенумеровывается, а не теряет номера", () => {
    const v = editor("1. первый\nвторой", 0, 16);
    toggleOrderedList(v);
    expect(textOf(v)).toBe("1. первый\n2. второй");
    v.destroy();
  });
});

describe("insertLink", () => {
  it("выделение становится подписью, каретка — внутри скобок", () => {
    const v = editor("сайт", 0, 4);
    insertLink(v, "[текст](url)");
    expect(textOf(v)).toBe("[сайт]()");
    expect(v.state.selection.main.head).toBe(7);
    v.destroy();
  });

  // The template is translated in the component and comes in as an argument, so
  // this module has no i18n dependency.
  it("без выделения вставляет переданный шаблон", () => {
    const v = editor("", 0);
    insertLink(v, "[text](url)");
    expect(textOf(v)).toBe("[text](url)");
    v.destroy();
  });
});

describe("insertTable", () => {
  it("на непустой строке отделяется пустой строкой", () => {
    const v = editor("текст", 5);
    insertTable(v);
    expect(textOf(v).startsWith("текст\n\n|")).toBe(true);
    v.destroy();
  });

  it("на пустой строке лишний отступ не добавляется", () => {
    const v = editor("", 0);
    insertTable(v);
    expect(textOf(v).startsWith("\n|")).toBe(true);
    v.destroy();
  });
});
