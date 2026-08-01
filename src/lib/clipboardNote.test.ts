import { describe, it, expect } from "vitest";
import { parseClipboardNote, TITLE_MAX } from "./clipboardNote";

describe("parseClipboardNote", () => {
  it("одна строка — заголовок без тела", () => {
    expect(parseClipboardNote("купить хлеб")).toEqual({
      title: "купить хлеб",
      content: "",
    });
  });

  it("первая строка — заголовок, остальное — тело", () => {
    expect(parseClipboardNote("Заголовок\nпервый абзац\nвторой абзац")).toEqual({
      title: "Заголовок",
      content: "первый абзац\nвторой абзац",
    });
  });

  // Text copied from a browser often starts with blank lines — without this the note
  // would get an empty title from a non-empty clipboard.
  it("ведущие пустые строки пропускаются, а не дают пустой заголовок", () => {
    expect(parseClipboardNote("\n\n  \nНастоящий заголовок\nтекст")).toEqual({
      title: "Настоящий заголовок",
      content: "текст",
    });
  });

  it("пустой буфер и пробельный буфер — null, а не пустая заметка", () => {
    expect(parseClipboardNote("")).toBeNull();
    expect(parseClipboardNote("   \n\n\t  ")).toBeNull();
  });

  it("длинный заголовок обрезается, тело не трогается", () => {
    const long = "я".repeat(300);
    const r = parseClipboardNote(`${long}\n${long}`)!;
    expect(r.title).toHaveLength(TITLE_MAX);
    expect(r.content).toHaveLength(300);
  });

  it("внутренние пустые строки в теле сохраняются", () => {
    const r = parseClipboardNote("Тема\n\nабзац один\n\nабзац два")!;
    expect(r.content).toBe("абзац один\n\nабзац два");
  });

  // A bare URL is unreadable as a title in the notes list and in wiki links, so such
  // a clipboard goes entirely into the body.
  it("скопированная ссылка идёт в тело, а не в заголовок", () => {
    expect(parseClipboardNote("https://example.com/a/b?x=1")).toEqual({
      title: "",
      content: "https://example.com/a/b?x=1",
    });
    expect(parseClipboardNote("www.example.com")).toEqual({
      title: "",
      content: "www.example.com",
    });
  });

  it("ссылка с текстом под ней — тело сохраняет обе строки", () => {
    expect(parseClipboardNote("https://example.com\nчто это за статья")).toEqual({
      title: "",
      content: "https://example.com\nчто это за статья",
    });
  });

  // The key difference from "the line contains a link": for a sentence with a link
  // inside, a title taken from the first line still makes sense.
  it("предложение со ссылкой внутри остаётся заголовком", () => {
    expect(parseClipboardNote("смотри https://example.com потом")).toEqual({
      title: "смотри https://example.com потом",
      content: "",
    });
  });

  it("текстовый заголовок со ссылкой во второй строке не меняется", () => {
    expect(parseClipboardNote("Статья про Rust\nhttps://example.com")).toEqual({
      title: "Статья про Rust",
      content: "https://example.com",
    });
  });

  it("CRLF из Windows-буфера не оставляет \\r в заголовке", () => {
    const r = parseClipboardNote("Заголовок\r\nтело")!;
    expect(r.title).toBe("Заголовок");
    expect(r.content).toBe("тело");
  });
});
