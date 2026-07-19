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
