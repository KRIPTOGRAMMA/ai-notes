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

  // Скопированное из браузера часто начинается с пустых строк — без этого
  // заметка получила бы пустой заголовок при непустом буфере.
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

  // Голый URL в заголовке нечитаем в списке заметок и в вики-ссылках —
  // такой буфер целиком уходит в тело.
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

  // Ключевое отличие от «в строке есть ссылка»: у предложения со ссылкой
  // внутри заголовок из первой строки по-прежнему осмысленный.
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
