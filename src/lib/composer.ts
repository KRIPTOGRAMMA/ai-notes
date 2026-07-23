// Парсер инлайн-композера задач. Текст из textarea, где Shift+Enter добавляет
// строку-подзадачу с префиксом «☐ »: первая обычная непустая строка — название,
// остальные обычные строки — описание, строки с «☐» — подзадачи.
// Чистая функция: UI вставляет префиксы, здесь только разбор.

export const SUBTASK_PREFIX = "☐ ";

export interface ComposerDraft {
  title: string;
  description: string;
  subtasks: string[];
}

export function parseComposer(src: string): ComposerDraft {
  const subtasks: string[] = [];
  const textLines: string[] = [];

  for (const line of (src ?? "").split("\n")) {
    if (line.trimStart().startsWith("☐")) {
      const t = line.trimStart().slice(1).trim();
      if (t) subtasks.push(t);
    } else {
      textLines.push(line);
    }
  }

  while (textLines.length > 0 && !textLines[0].trim()) textLines.shift();
  const title = (textLines.shift() ?? "").trim();
  const description = textLines.join("\n").trim();

  return { title, description, subtasks };
}

// --- Естественный язык в названии задачи (v0.9.17) ---
// «завтра 15:00 созвон !высокий @работа #важное» → title "созвон" + метаданные.
// Токены распознаются только в строке названия (title из parseComposer), не в
// описании/подзадачах — естественная граница: остальной текст остаётся как есть.
//
// Синтаксис маркеров (не пересекается с уже существующим #tag на задачах):
//   !маркер   — приоритет (низкий/средний/высокий/критический + синонимы)
//   @маркер   — категория (сопоставляется по имени категории, регистр не важен)
//   #маркер   — тег (как и раньше, просто добавляется в tags)
//   дата/время — "завтра", "послезавтра", "сегодня", день недели, "HH:MM" —
//                любая комбинация даты и времени, в любом порядке относительно текста

export interface ParsedTaskMeta {
  title: string;
  priority: "Low" | "Medium" | "High" | "Critical" | null;
  categoryQuery: string | null; // сырое слово после @ — сопоставление с categoryStore снаружи
  tags: string[];
  deadline: Date | null;
}

const PRIORITY_WORDS: Record<string, ParsedTaskMeta["priority"]> = {
  "низкий": "Low", "низк": "Low",
  "средний": "Medium", "средн": "Medium", "норм": "Medium", "обычный": "Medium",
  "высокий": "High", "высок": "High", "срочно": "High", "важно": "High",
  "критический": "Critical", "критично": "Critical", "критик": "Critical",
};

const WEEKDAYS: Record<string, number> = {
  "понедельник": 1, "вторник": 2, "среда": 3, "среду": 3, "четверг": 4,
  "пятница": 5, "пятницу": 5, "суббота": 6, "субботу": 6, "воскресенье": 0,
};

function matchPriority(word: string): ParsedTaskMeta["priority"] {
  const norm = word.toLowerCase().replace(/[^a-zа-яё]/gi, "");
  for (const [key, value] of Object.entries(PRIORITY_WORDS)) {
    if (norm.startsWith(key)) return value;
  }
  return null;
}

// Ближайшая дата с этим днём недели (0=вс), включая сегодня, если совпадает
// и время ещё не прошло — иначе следующая неделя. Упрощённо: всегда следующее
// вхождение, не считая "сегодня" отдельным случаем (пользователь скажет "сегодня").
function nextWeekday(from: Date, targetDow: number): Date {
  const d = new Date(from);
  const diff = (targetDow - d.getDay() + 7) % 7 || 7;
  d.setDate(d.getDate() + diff);
  return d;
}

function applyTime(d: Date, hh: number, mm: number): Date {
  const out = new Date(d);
  out.setHours(hh, mm, 0, 0);
  return out;
}

export function parseTaskText(rawTitle: string, now: Date = new Date()): ParsedTaskMeta {
  const tokens = rawTitle.split(/\s+/).filter(Boolean);
  const titleWords: string[] = [];
  let priority: ParsedTaskMeta["priority"] = null;
  let categoryQuery: string | null = null;
  const tags: string[] = [];

  let datePart: Date | null = null; // только дата (00:00), выставляется словами дня
  let timeHH: number | null = null;
  let timeMM: number | null = null;

  for (const token of tokens) {
    if (token.startsWith("!") && token.length > 1) {
      const p = matchPriority(token.slice(1));
      if (p) { priority = p; continue; }
    }
    if (token.startsWith("@") && token.length > 1) {
      categoryQuery = token.slice(1);
      continue;
    }
    if (token.startsWith("#") && token.length > 1) {
      tags.push(token.slice(1));
      continue;
    }

    const timeMatch = /^(\d{1,2}):(\d{2})$/.exec(token);
    if (timeMatch) {
      const hh = Number(timeMatch[1]);
      const mm = Number(timeMatch[2]);
      if (hh <= 23 && mm <= 59) {
        timeHH = hh;
        timeMM = mm;
        continue;
      }
    }

    const lower = token.toLowerCase().replace(/[.,!?]+$/, "");
    if (lower === "сегодня") {
      datePart = applyTime(now, 0, 0);
      continue;
    }
    if (lower === "завтра") {
      datePart = applyTime(now, 0, 0);
      datePart.setDate(datePart.getDate() + 1);
      continue;
    }
    if (lower === "послезавтра") {
      datePart = applyTime(now, 0, 0);
      datePart.setDate(datePart.getDate() + 2);
      continue;
    }
    if (lower in WEEKDAYS) {
      datePart = applyTime(nextWeekday(now, WEEKDAYS[lower]), 0, 0);
      continue;
    }

    titleWords.push(token);
  }

  let deadline: Date | null = null;
  if (datePart || timeHH !== null) {
    const base = datePart ?? new Date(now);
    deadline = timeHH !== null ? applyTime(base, timeHH, timeMM ?? 0) : base;
  }

  return {
    title: titleWords.join(" ").trim(),
    priority,
    categoryQuery,
    tags,
    deadline,
  };
}

// Сопоставление @категория с существующими категориями по имени/id — тот же
// нормализующий принцип, что match_category на бэкенде (commands/categories.rs)
// для ИИ-классификации: подрезаем пунктуацию по краям, без учёта регистра.
export function matchCategoryQuery(
  categories: { id: string; name: string }[],
  query: string,
): string | null {
  const norm = query.trim().replace(/^[^a-zа-яё0-9]+|[^a-zа-яё0-9]+$/gi, "").toLowerCase();
  if (!norm) return null;
  const found = categories.find(
    c => c.name.toLowerCase() === norm || c.id.toLowerCase() === norm,
  );
  return found?.id ?? null;
}
