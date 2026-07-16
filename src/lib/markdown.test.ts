import { describe, it, expect } from "vitest";
import { renderMarkdown, taskLineIndices, toggleTaskListItem, extractWikiLinks } from "./markdown";

describe("renderMarkdown", () => {
  it("рендерит gfm-чекбоксы", () => {
    const html = renderMarkdown("- [ ] один\n- [x] два");
    expect(html).toContain('type="checkbox"');
    expect((html.match(/type="checkbox"/g) ?? []).length).toBe(2);
    expect(html).toContain("checked");
  });

  it("санитизирует опасный HTML", () => {
    const html = renderMarkdown('текст <img src=x onerror="alert(1)"> <script>alert(2)</script>');
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("<script>");
    expect(html).toContain("текст");
  });

  it("пустой и null-вход не падают", () => {
    expect(renderMarkdown("")).toBe("");
    expect(renderMarkdown(null as unknown as string)).toBe("");
  });

  it("рендерит вики-ссылку с data-wikilink", () => {
    const html = renderMarkdown("см. [[Моя заметка]]");
    expect(html).toContain('class="wikilink"');
    expect(html).toContain('data-wikilink="Моя заметка"');
    expect(html).toContain(">Моя заметка</a>");
  });

  it("вики-ссылка с алиасом: текст — алиас, цель — название", () => {
    const html = renderMarkdown("[[Идея|вот тут]]");
    expect(html).toContain('data-wikilink="Идея"');
    expect(html).toContain(">вот тут</a>");
  });

  it("[[...]] внутри кода остаётся текстом", () => {
    const inline = renderMarkdown("код `[[не ссылка]]`");
    expect(inline).not.toContain("wikilink");
    const block = renderMarkdown("```\n[[тоже не ссылка]]\n```");
    expect(block).not.toContain("wikilink");
  });

  it("HTML в названии ссылки не становится элементом", () => {
    const html = renderMarkdown('[[<img src=x onerror="alert(1)">]]');
    const doc = new DOMParser().parseFromString(html, "text/html");
    expect(doc.querySelector("img")).toBeNull();
    // Название целиком ушло в data-атрибут как строка
    expect(doc.querySelector("a.wikilink")?.getAttribute("data-wikilink"))
      .toBe('<img src=x onerror="alert(1)">');
  });
});

describe("extractWikiLinks", () => {
  it("извлекает названия, алиас отбрасывает, дубли схлопывает без учёта регистра", () => {
    expect(extractWikiLinks("[[А]] и [[Б|текст]], снова [[а]]")).toEqual(["А", "Б"]);
  });

  it("одиночные скобки и пустые ссылки игнорирует", () => {
    expect(extractWikiLinks("[не ссылка] [[]] [[  ]] текст")).toEqual([]);
    expect(extractWikiLinks("")).toEqual([]);
  });

  it("тримит пробелы вокруг названия", () => {
    expect(extractWikiLinks("[[ Заметка ]]")).toEqual(["Заметка"]);
  });
});

describe("taskLineIndices", () => {
  it("находит только строки-чекбоксы, в порядке следования", () => {
    const src = [
      "# Заголовок",
      "- [ ] первый",
      "- обычный пункт",
      "  * [X] вложенный, звёздочка, заглавная X",
      "+ [x] плюс-маркер",
      "не список [ ]",
    ].join("\n");
    expect(taskLineIndices(src)).toEqual([1, 3, 4]);
  });

  it("пустой текст — пусто", () => {
    expect(taskLineIndices("")).toEqual([]);
  });
});

describe("toggleTaskListItem", () => {
  const src = "- [ ] один\nтекст\n- [x] два";

  it("ставит и снимает галочку по индексу чекбокса (не строки)", () => {
    expect(toggleTaskListItem(src, 0)).toBe("- [x] один\nтекст\n- [x] два");
    expect(toggleTaskListItem(src, 1)).toBe("- [ ] один\nтекст\n- [ ] два");
  });

  it("сохраняет отступ и маркер списка", () => {
    expect(toggleTaskListItem("  * [ ] пункт", 0)).toBe("  * [x] пункт");
  });

  it("индекс вне диапазона возвращает исходник без изменений", () => {
    expect(toggleTaskListItem(src, 5)).toBe(src);
    expect(toggleTaskListItem(src, -1)).toBe(src);
  });
});
