import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({ gfm: true, breaks: true });

// Рендер Markdown в безопасный HTML. Санитизация обязательна: контент может
// прийти из импорта/вставки, а не только из ручного ввода.
export function renderMarkdown(src: string): string {
  const raw = marked.parse(src ?? "", { async: false }) as string;
  return DOMPurify.sanitize(raw);
}

const TASK_LINE = /^(\s*[-*+]\s+)\[( |x|X)\]/;

// Индексы строк editContent, содержащих markdown-чекбокс, по порядку. Порядок
// совпадает с порядком <input type=checkbox> в отрендеренном HTML (gfm task list).
export function taskLineIndices(src: string): number[] {
  const out: number[] = [];
  const lines = src.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (TASK_LINE.test(lines[i])) out.push(i);
  }
  return out;
}

// Переключает N-й (по порядку) чекбокс в markdown-тексте: `- [ ]` ↔ `- [x]`.
// Чистая функция — возвращает новый текст; вне диапазона возвращает исходный.
export function toggleTaskListItem(src: string, checkboxIndex: number): string {
  const lines = src.split("\n");
  const indices = taskLineIndices(src);
  const lineNo = indices[checkboxIndex];
  if (lineNo === undefined) return src;
  lines[lineNo] = lines[lineNo].replace(TASK_LINE, (_m, prefix: string, mark: string) =>
    `${prefix}[${mark === " " ? "x" : " "}]`
  );
  return lines.join("\n");
}
