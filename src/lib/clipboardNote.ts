// Parsing clipboard text into a note.
//
// Extracted from QuickCapture.svelte into its own module because vitest in this
// project covers pure ts only (vitest.config.ts) — the same approach as guard.ts.
// What lives here is logic rather than markup: exactly how the text splits into a
// title and a body, and what counts as an empty clipboard.

// A note's title must not be a wall of text: the clipboard has no length limit,
// while the title appears in lists and wiki links.
export const TITLE_MAX = 120;

export type ClipboardNote = { title: string; content: string };

// Whether the line is entirely a link. We check "the line IS a URL" rather than
// "the line contains one": for a sentence with a link inside, a title taken from
// the first line still makes sense, whereas for a bare URL it does not.
function isBareUrl(line: string): boolean {
  return /^(https?:\/\/|www\.)\S+$/i.test(line.trim());
}

// An empty clipboard (or an image or file, for which the plugin returns an empty
// string) yields null rather than an empty note: the caller opens an ordinary
// blank form.
export function parseClipboardNote(raw: string): ClipboardNote | null {
  if (!raw.trim()) return null;

  // The first non-empty line is the title and the rest is the body. Leading blank
  // lines are skipped: text copied from a browser often starts with them, and
  // otherwise the note would get an empty title from a non-empty clipboard.
  const lines = raw.split("\n");
  let i = 0;
  while (i < lines.length && !lines[i].trim()) i++;

  const first = lines[i].trim();

  // A bare URL does not become a title: it is unreadable in the notes list and in
  // `[[...]]` wiki links. Such a clipboard goes entirely into the body and the title
  // stays empty — the user writes their own (or the "Untitled" fallback applies on
  // save).
  if (isBareUrl(first)) {
    return { title: "", content: lines.slice(i).join("\n").trim() };
  }

  const title = first.slice(0, TITLE_MAX);
  const content = lines.slice(i + 1).join("\n").trim();
  return { title, content };
}
