// Проверка ссылки перед открытием во внешнем браузере (v0.9.27).
//
// Markdown в заметке — это произвольный текст: он мог быть вставлен из
// буфера, прийти от ИИ или из импортированного .md. Отдавать такую строку
// в openUrl() без проверки схемы нельзя — `javascript:` и `data:` это
// исполняемый код, `file:` — доступ к локальной ФС.
//
// Отдельный чистый модуль, потому что vitest покрывает только чистые ts
// (см. тот же приём в guard.ts и clipboardNote.ts).

// Белый список, а не чёрный: неизвестная схема безопаснее считается
// небезопасной, иначе каждая новая экзотическая схема — дыра.
const SAFE_SCHEMES = ["http:", "https:", "mailto:"];

export function isSafeUrl(raw: string): boolean {
  const trimmed = raw.trim();
  if (!trimmed) return false;

  // Ссылка без схемы (example.com, /path, #anchor) — не абсолютный URL,
  // открывать её во внешнем браузере нечем. Считаем небезопасной, но по
  // другой причине: не «опасно», а «некуда вести».
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return false;
  }

  // toLowerCase: `JavaScript:` — та же схема, что `javascript:`.
  return SAFE_SCHEMES.includes(url.protocol.toLowerCase());
}
