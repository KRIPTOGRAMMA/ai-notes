import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({ gfm: true, breaks: true });

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// Вики-ссылки: [[Название]] или [[Название|текст]]. Расширение marked, а не
// пре-процессинг текста — так [[...]] внутри `кода` и ```блоков``` остаётся
// текстом. Резолвинг по названию делает UI (data-wikilink), рендер лишь метит.
const WIKILINK_RE = /^\[\[([^\[\]|]+)(?:\|([^\[\]]+))?\]\]/;

marked.use({
  extensions: [
    {
      name: "wikilink",
      level: "inline",
      start(src: string) {
        const i = src.indexOf("[[");
        return i < 0 ? undefined : i;
      },
      tokenizer(src: string) {
        const m = WIKILINK_RE.exec(src);
        if (!m) return undefined;
        const title = m[1].trim();
        if (!title) return undefined;
        return {
          type: "wikilink",
          raw: m[0],
          title,
          label: (m[2] ?? m[1]).trim(),
        };
      },
      renderer(token) {
        const t = token as unknown as { title: string; label: string };
        return `<a href="#" class="wikilink" data-wikilink="${escapeHtml(t.title)}">${escapeHtml(t.label)}</a>`;
      },
    },
  ],
});

// Названия заметок, на которые ссылается текст (для бэклинков). Работает по
// сырому markdown — ссылки в code-блоках тоже попадут, для бэклинков это ок.
export function extractWikiLinks(src: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const m of (src ?? "").matchAll(/\[\[([^\[\]|]+)(?:\|[^\[\]]+)?\]\]/g)) {
    const title = m[1].trim();
    const key = title.toLowerCase();
    if (title && !seen.has(key)) {
      seen.add(key);
      out.push(title);
    }
  }
  return out;
}

// Рендер Markdown в безопасный HTML. Санитизация обязательна: контент может
// прийти из импорта/вставки, а не только из ручного ввода.
export function renderMarkdown(src: string): string {
  const raw = marked.parse(src ?? "", { async: false }) as string;
  return DOMPurify.sanitize(raw);
}

// Картинки ![alt](filename) — filename без пути (то, что вернул save_note_image).
// Общий regex для парсинга (LiveMarkdownEditor) и построения markdown при вставке.
export const IMAGE_RE = /!\[([^\[\]]*)\]\(([^()\s]+)\)/g;

export function imageMarkdown(filename: string): string {
  return `![](${filename})`;
}

// Расширение картинки для save_note_image: из MIME-типа (image/png → png,
// image/jpeg → jpg) или, если MIME отсутствует, из имени файла; дефолт — png.
export function extImageExt(mimeOrName: string): string {
  const fromMime = /^image\/([a-z0-9]+)/i.exec(mimeOrName)?.[1];
  if (fromMime) return fromMime === "jpeg" ? "jpg" : fromMime;
  return (/\.([a-z0-9]+)$/i.exec(mimeOrName)?.[1] ?? "png").toLowerCase();
}

// --- Таблицы (v0.9.06) ---
// Простой построчный парсер GFM pipe-таблиц — не завязан на Lezer-дерево,
// т.к. виджету редактора нужен полный контроль над границами ячеек при
// сериализации обратно в текст (Lezer даёт позиции для подсветки, но не для
// надёжной round-trip пересборки при редактировании отдельной ячейки).
export type TableAlign = "left" | "center" | "right" | null;
export interface ParsedTable {
  header: string[];
  align: TableAlign[];
  rows: string[][];
}

const DELIMITER_CELL = /^:?-+:?$/;

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\" && s[i + 1] === "|") { cur += "|"; i++; continue; }
    if (c === "|") { cells.push(cur.trim()); cur = ""; continue; }
    cur += c;
  }
  cells.push(cur.trim());
  return cells;
}

function parseAlign(cell: string): TableAlign {
  const left = cell.startsWith(":");
  const right = cell.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return null;
}

// Пытается разобрать таблицу, начинающуюся на строке `startLine` (1-based).
// Возвращает null, если это не таблица (нет разделительной строки вида
// | --- | :--: | сразу после заголовка) — тогда вызывающий код просто не
// рендерит виджет, текст остаётся обычным абзацем.
export function parseTableAt(doc: string, startLine: number): { table: ParsedTable; endLine: number } | null {
  const lines = doc.split("\n");
  const header = lines[startLine - 1];
  const delim = lines[startLine];
  if (header === undefined || delim === undefined) return null;
  if (!header.includes("|")) return null;
  const delimCells = splitRow(delim);
  if (delimCells.length === 0 || !delimCells.every(c => DELIMITER_CELL.test(c))) return null;

  const headerCells = splitRow(header);
  const align = delimCells.map(parseAlign);
  const rows: string[][] = [];
  let i = startLine + 1;
  while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
    rows.push(splitRow(lines[i]));
    i++;
  }
  return { table: { header: headerCells, align, rows }, endLine: i };
}

// Сериализация обратно в markdown — выравнивает столбцы пробелами для
// читаемости сырого текста (не обязательно для GFM, но так таблицу приятно
// видеть и вне live-preview, напр. при экспорте в .md).
export function serializeTable(table: ParsedTable): string {
  const cols = table.header.length;
  const widths = Array.from({ length: cols }, (_, c) => {
    const cellLens = table.rows.map(r => (r[c] ?? "").length);
    return Math.max(3, table.header[c]?.length ?? 0, ...cellLens);
  });
  const pad = (s: string, w: number, a: TableAlign) => {
    const gap = Math.max(0, w - s.length);
    if (a === "right") return " ".repeat(gap) + s;
    if (a === "center") {
      const left = Math.floor(gap / 2);
      return " ".repeat(left) + s + " ".repeat(gap - left);
    }
    return s + " ".repeat(gap);
  };
  const row = (cells: string[]) =>
    "| " + cells.map((c, i) => pad(c ?? "", widths[i], table.align[i] ?? null)).join(" | ") + " |";
  // Разделительная строка: дефисы заполняют ширину столбца, двоеточия
  // выравнивания остаются на своих краях (":--", "--:", ":--:") — так
  // маркер остаётся однозначно читаемым при любой ширине столбца.
  const delimCell = (w: number, a: TableAlign) => {
    const left = a === "left" || a === "center" ? ":" : "";
    const right = a === "right" || a === "center" ? ":" : "";
    const dashes = Math.max(1, w - left.length - right.length);
    return left + "-".repeat(dashes) + right;
  };
  const delim = "| " + widths.map((w, i) => delimCell(w, table.align[i] ?? null)).join(" | ") + " |";
  return [row(table.header), delim, ...table.rows.map(row)].join("\n");
}

export function emptyTable(cols: number, rows: number): ParsedTable {
  return {
    header: Array.from({ length: cols }, (_, i) => `Колонка ${i + 1}`),
    align: Array.from({ length: cols }, () => null),
    rows: Array.from({ length: rows }, () => Array.from({ length: cols }, () => "")),
  };
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
