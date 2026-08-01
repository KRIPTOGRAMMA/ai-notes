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

// Wiki links: [[Title]] or [[Title|text]]. A marked extension rather than text
// pre-processing, so [[...]] inside `code` and ```blocks``` stays text. Resolving by
// title is the UI's job (data-wikilink); the renderer only tags them.
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

// The titles of notes the text links to (for backlinks). It works over the raw
// markdown, so links inside code blocks are included too — fine for backlinks.
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

// Renders Markdown into safe HTML. Sanitization is mandatory: the content may come
// from an import or a paste rather than only from manual typing.
export function renderMarkdown(src: string): string {
  const raw = marked.parse(src ?? "", { async: false }) as string;
  return DOMPurify.sanitize(raw);
}

// Images ![alt](filename), where filename carries no path (what save_note_image
// returned). A shared regex for parsing (LiveMarkdownEditor) and for building the
// markdown on paste.
export const IMAGE_RE = /!\[([^\[\]]*)\]\(([^()\s]+)\)/g;

export function imageMarkdown(filename: string): string {
  return `![](${filename})`;
}

// The image extension for save_note_image: from the MIME type (image/png -> png,
// image/jpeg -> jpg) or, when the MIME type is absent, from the filename; the
// default is png.
export function extImageExt(mimeOrName: string): string {
  const fromMime = /^image\/([a-z0-9]+)/i.exec(mimeOrName)?.[1];
  if (fromMime) return fromMime === "jpeg" ? "jpg" : fromMime;
  return (/\.([a-z0-9]+)$/i.exec(mimeOrName)?.[1] ?? "png").toLowerCase();
}

// --- Tables ---
// A simple line-by-line parser for GFM pipe tables, not tied to the Lezer tree:
// the editor widget needs full control over cell boundaries when serializing back
// to text (Lezer gives positions for highlighting, but not for a reliable
// round-trip rebuild when a single cell is edited).
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

// Tries to parse a table starting at line `startLine` (1-based). Returns null if it
// is not a table (no separator row of the form | --- | :--: | right after the
// header), in which case the caller simply does not render the widget and the text
// stays an ordinary paragraph.
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

// Serialization back into markdown, padding the columns with spaces for the sake of
// the raw text's readability (not required by GFM, but it makes the table pleasant
// to look at outside live preview too, for instance when exported to .md).
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
  // The separator row: hyphens fill the column's width and the alignment colons stay
  // at their edges (":--", "--:", ":--:"), so the marker remains unambiguous at any
  // column width.
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

// The indices of editContent's lines containing a markdown checkbox, in order. That
// order matches the order of <input type=checkbox> in the rendered HTML (a GFM task
// list).
export function taskLineIndices(src: string): number[] {
  const out: number[] = [];
  const lines = src.split("\n");
  for (let i = 0; i < lines.length; i++) {
    if (TASK_LINE.test(lines[i])) out.push(i);
  }
  return out;
}

// Toggles the Nth checkbox (in order) in the markdown text: `- [ ]` <-> `- [x]`.
// A pure function returning new text; out of range it returns the original.
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
