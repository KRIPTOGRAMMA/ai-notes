import { describe, it, expect } from "vitest";
import { parseComposer } from "./composer";

describe("parseComposer", () => {
  it("одна строка — только название", () => {
    expect(parseComposer("купить хлеб")).toEqual({
      title: "купить хлеб",
      description: "",
      subtasks: [],
    });
  });

  it("обычные строки после названия — описание, ☐-строки — подзадачи", () => {
    const src = "составить план\nна следующую неделю\n☐ пункт один\n☐ пункт два";
    expect(parseComposer(src)).toEqual({
      title: "составить план",
      description: "на следующую неделю",
      subtasks: ["пункт один", "пункт два"],
    });
  });

  it("подзадачи могут идти вперемешку с текстом, порядок сохраняется", () => {
    const src = "задача\n☐ раз\nописание\n☐ два";
    const d = parseComposer(src);
    expect(d.subtasks).toEqual(["раз", "два"]);
    expect(d.description).toBe("описание");
  });

  it("пустые ☐-строки и ведущие пустые строки отбрасываются", () => {
    const src = "\n\nзадача\n☐ \n☐ реальная";
    expect(parseComposer(src)).toEqual({
      title: "задача",
      description: "",
      subtasks: ["реальная"],
    });
  });

  it("☐ с отступом тоже подзадача", () => {
    expect(parseComposer("t\n  ☐ вложенная").subtasks).toEqual(["вложенная"]);
  });

  it("пустой и null-вход не падают", () => {
    expect(parseComposer("")).toEqual({ title: "", description: "", subtasks: [] });
    expect(parseComposer(null as unknown as string).title).toBe("");
  });
});
