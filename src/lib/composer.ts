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
