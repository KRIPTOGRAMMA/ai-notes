import { describe, it, expect } from "vitest";

// The guard for "all code comments are in English".
//
// The rule itself was carried out over v0.9.57–v0.9.61 and declared complete, yet
// the hole reopened eight times: every sweep searched by an incomplete pattern.
// It knew only `//` and missed trailing comments; then it missed SQL `--`; then
// CSS `/* */`; then continuation lines of multi-line blocks (only the first line
// got translated, producing hybrids that LOOK translated); then standalone .css
// files, which no glob covered; and finally the root config files, which no sweep
// ever looked at. Nine of the remaining lines were written by the author of this
// very guard, during the three versions right before it.
//
// A grep is not enough for the same reason it kept failing: it has to be re-typed
// correctly every time. This test is the pattern, written down once.
//
// Note the scope: `e2e/*.spec.ts` is deliberately NOT covered — those tests are
// written in Russian by convention, names and comments alike.

// NOTE: .css is deliberately absent. Vite runs stylesheets through its own
// pipeline, so import.meta.glob returns an EMPTY string for them under every form
// of ?raw — the path is listed, the content is not, and the guard would silently
// scan nothing. That is exactly the failure this task exists to prevent, so the
// CSS half lives on the Rust side (i18n.rs::no_russian_comments_in_css) where
// include_str! can actually read the file.
const SOURCES: Record<string, string> = {
  ...import.meta.glob("/src/**/*.{ts,svelte}", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("/src-tauri/src/**/*.rs", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("/*.config.ts", { query: "?raw", import: "default", eager: true }),
} as Record<string, string>;

// A comment may legitimately quote Cyrillic: it is the subject being explained,
// not the language of the explanation. Real examples that must NOT be flagged:
//   // New tasks go to the end: а, б, в, г
//   // ...messages carry ": " too — "Недопустимое расширение: png"
//   // A self-link [[Идея]] -> [[X]] is rewritten like any other link
// So quoted spans are cut out before the Cyrillic test runs.
const QUOTED = /"[^"]*"|'[^']*'|`[^`]*`|«[^»]*»|\[\[[^\]]*\]\]/g;

// The two lines where bare Cyrillic is the example itself, with no quotes to strip
// (a, b, c enumerations in Russian letters). A heuristic like "N Cyrillic words in
// a row" would need N >= 5 to clear them, which is loose enough to let real Russian
// comments through — an explicit marker is worth more than a tuning knob. Mirrors
// the existing /* i18n-ok */ convention in App.svelte.
const MARKER = "ru-ok";

// Every comment syntax used in this project, including trailing ones and the
// continuation lines of multi-line blocks — those were holes 1, 4 and 5.
function commentLines(src: string): { line: number; text: string }[] {
  const out: { line: number; text: string }[] = [];
  const lines = src.split("\n");
  let inBlock = false;
  lines.forEach((rawLine, i) => {
    // String literals are blanked out (keeping the line length, so indices still
    // line up) BEFORE looking for a comment opener: without this the "//" inside
    // a URL like "https://example.com" reads as the start of a comment.
    const raw = rawLine.replace(/"[^"]*"|'[^']*'|`[^`]*`/g, m => " ".repeat(m.length));
    let text = "";
    if (inBlock) {
      text = rawLine;
      if (raw.includes("*/")) inBlock = false;
    } else {
      const block = raw.indexOf("/*");
      const html = raw.indexOf("<!--");
      const slash = raw.indexOf("//");
      if (block >= 0) {
        text = rawLine.slice(block);
        if (!raw.includes("*/", block + 2)) inBlock = true;
      } else if (html >= 0) {
        text = rawLine.slice(html);
      } else if (slash >= 0) {
        // A trailing comment counts too: hole 1 was exactly the assumption that a
        // comment always starts the line.
        text = rawLine.slice(slash);
      }
    }
    if (text.trim()) out.push({ line: i + 1, text });
  });
  return out;
}

describe("комментарии в коде", () => {
  // Checks the CONTENT, not just the path list. Listing a path proves nothing:
  // .css files were listed by the glob while their content came back empty, so a
  // guard built on paths alone reported success over an unread file.
  it("глоб достаёт непустое содержимое всех областей", () => {
    const nonEmpty = (pred: (p: string) => boolean) =>
      Object.entries(SOURCES).filter(([p]) => pred(p)).filter(([, src]) => src.trim().length > 0);

    expect(nonEmpty(p => p.endsWith(".rs")).length, "не читает Rust").toBeGreaterThan(10);
    expect(nonEmpty(p => p.endsWith(".svelte")).length, "не читает Svelte").toBeGreaterThan(10);
    expect(nonEmpty(p => p.endsWith(".ts")).length, "не читает TypeScript").toBeGreaterThan(10);
    expect(nonEmpty(p => p.startsWith("/vitest.config")).length, "не читает корневые конфиги").toBe(1);
  });

  it("нет русских комментариев ни в одном синтаксисе", () => {
    const offenders: string[] = [];
    for (const [path, src] of Object.entries(SOURCES)) {
      const lines = commentLines(src);
      lines.forEach(({ line, text }, idx) => {
        if (text.includes(MARKER)) return;
        // A quoted example may be wrapped across two lines ("Недопустимое\n
        // расширение: png"), and then neither line has a balanced pair of quotes.
        // So the neighbours of a contiguous comment block are glued back together
        // before the quotes are cut — this is the block-level view that hole 5
        // (multi-line continuations) taught us to take.
        const prev = idx > 0 && lines[idx - 1].line === line - 1 ? lines[idx - 1].text : "";
        const next = idx + 1 < lines.length && lines[idx + 1].line === line + 1 ? lines[idx + 1].text : "";
        const joined = `${prev} ${text} ${next}`;
        // The order matters: cut the quotes out FIRST, then look for Cyrillic.
        if (/[а-яА-ЯёЁ]/.test(text.replace(QUOTED, "")) && /[а-яА-ЯёЁ]/.test(joined.replace(QUOTED, ""))) {
          offenders.push(`${path}:${line}  ${text.trim().slice(0, 70)}`);
        }
      });
    }
    expect(offenders).toEqual([]);
  });
});
