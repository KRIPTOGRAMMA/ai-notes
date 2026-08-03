// Editing operations for the note editor.
//
// Extracted from LiveMarkdownEditor.svelte, where they could not be unit-tested:
// vitest here has no svelte plugin, so anything living next to a $state rune is
// out of reach (the same reason guard.ts and datetime.ts are separate modules).
// The view comes in as the first argument instead of being closed over, which is
// the whole difference from the previous versions of these functions.
//
// The component keeps one-line delegates with the same names and signatures, so
// its public surface (EditorExports in Notes.svelte) is unchanged.

import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { serializeTable, emptyTable } from "../markdown";

export function replaceRange(view: EditorView, from: number, to: number, text: string) {
  view.dispatch({
    changes: { from, to, insert: text },
    selection: EditorSelection.cursor(from + text.length),
    scrollIntoView: true,
  });
  view.focus();
}

// Inserts text where the cursor is, replacing the selection if there is one —
// the same thing typing would do. Used by voice input: dictation is just another
// way of entering text, so it must not care whether the caret sits mid-word or a
// paragraph is selected.
//
// Spaces are added on whichever side abuts a word character: whisper returns a
// bare phrase with no padding, so dictating with the caret mid-sentence would
// otherwise glue the phrase to the neighbouring word on that side.
export function insertAtCursor(view: EditorView, text: string) {
  if (!text) return;
  const { state } = view;
  const range = state.selection.main;
  const charBefore = range.from > 0 ? state.doc.sliceString(range.from - 1, range.from) : "";
  const charAfter = range.to < state.doc.length ? state.doc.sliceString(range.to, range.to + 1) : "";
  const padLeft = charBefore !== "" && !/\s/.test(charBefore);
  const padRight = charAfter !== "" && !/\s/.test(charAfter);
  const insert = (padLeft ? " " : "") + text + (padRight ? " " : "");
  view.dispatch({
    changes: { from: range.from, to: range.to, insert },
    selection: EditorSelection.cursor(range.from + insert.length),
    scrollIntoView: true,
  });
  view.focus();
}

// Formatting from the external toolbar: wraps the selection in markers (bold,
// italics, code) or toggles a line prefix (heading, checklist). It works without
// a selection too, inserting an empty pair of markers with the cursor inside
// (bold, italics, code) or simply adding the prefix on the current line
// (heading, checklist, wiki link).
// Pressing again on already-wrapped text unwraps it: otherwise Ctrl+B on **bold**
// text would keep piling extra ** on the outside (the standard toggle-formatting
// behaviour of any text editor).
export function wrapSelection(view: EditorView, before: string, after: string) {
  const { state } = view;
  const changes = state.changeByRange(range => {
    const selected = state.sliceDoc(range.from, range.to);
    const alreadyWrapped = !range.empty
      && selected.startsWith(before) && selected.endsWith(after)
      && selected.length >= before.length + after.length;
    if (alreadyWrapped) {
      const inner = selected.slice(before.length, selected.length - after.length);
      return {
        changes: [{ from: range.from, to: range.to, insert: inner }],
        range: EditorSelection.range(range.from, range.from + inner.length),
      };
    }
    const insertBefore = { from: range.from, insert: before };
    const insertAfter = { from: range.to, insert: after };
    return {
      changes: [insertBefore, insertAfter],
      range: range.empty
        ? EditorSelection.cursor(range.from + before.length)
        : EditorSelection.range(range.from + before.length, range.to + before.length),
    };
  });
  view.dispatch(state.update(changes, { scrollIntoView: true }));
  view.focus();
}

export function toggleLinePrefix(view: EditorView, prefix: string) {
  const { state } = view;
  const changes = state.changeByRange(range => {
    const line = state.doc.lineAt(range.from);
    const has = line.text.startsWith(prefix);
    const change = has
      ? { from: line.from, to: line.from + prefix.length, insert: "" }
      : { from: line.from, insert: prefix };
    const delta = has ? -prefix.length : prefix.length;
    return {
      changes: [change],
      range: EditorSelection.range(range.from + delta, range.to + delta),
    };
  });
  view.dispatch(state.update(changes, { scrollIntoView: true }));
  view.focus();
}

// An ordered list cannot go through toggleLinePrefix: that takes a static prefix,
// while here every line has its own number. We number the selected lines from 1;
// if the list already exists, we remove it.
export function toggleOrderedList(view: EditorView) {
  const { state } = view;
  const range = state.selection.main;
  const first = state.doc.lineAt(range.from).number;
  const last = state.doc.lineAt(range.to).number;

  const NUM_RE = /^(\s*)\d+\.\s+/;
  // The numbering is removed only if EVERY non-empty line has it, or a click on a
  // partially formatted block would silently lose the numbers.
  let allNumbered = true;
  for (let n = first; n <= last; n++) {
    const t = state.doc.line(n).text;
    if (t.trim() && !NUM_RE.test(t)) { allNumbered = false; break; }
  }

  const changes: { from: number; to: number; insert: string }[] = [];
  let counter = 1;
  for (let n = first; n <= last; n++) {
    const line = state.doc.line(n);
    if (!line.text.trim()) continue; // blank lines are not numbered
    if (allNumbered) {
      const m = NUM_RE.exec(line.text)!;
      changes.push({ from: line.from, to: line.from + m[0].length, insert: m[1] });
    } else {
      const m = NUM_RE.exec(line.text);
      // An already-numbered line is renumbered rather than prefixed again
      const from = line.from;
      const to = m ? line.from + m[0].length : line.from;
      changes.push({ from, to, insert: `${counter}. ` });
    }
    counter++;
  }
  if (changes.length === 0) return;
  view.dispatch({ changes, scrollIntoView: true });
  view.focus();
}

// An ordinary link [text](url): the selection becomes the label and the cursor
// lands inside the empty parentheses, so the url is typed at once without a
// second click. With no selection a template is inserted with the cursor on the
// word "text" — the template is translated, so it arrives as an argument and this
// module stays free of i18n.
export function insertLink(view: EditorView, emptyTemplate: string) {
  const { state } = view;
  const range = state.selection.main;
  const label = state.sliceDoc(range.from, range.to);

  if (label) {
    const insert = `[${label}]()`;
    view.dispatch({
      changes: { from: range.from, to: range.to, insert },
      selection: EditorSelection.cursor(range.from + insert.length - 1),
      scrollIntoView: true,
    });
  } else {
    view.dispatch({
      changes: { from: range.from, insert: emptyTemplate },
      selection: EditorSelection.range(range.from + 1, range.from + 6),
      scrollIntoView: true,
    });
  }
  view.focus();
}

// Inserting a table: a starter 2x2 table on a new line below the cursor. The blank
// lines before and after are not because parseTableAt requires them (it does not,
// a table parses even flush against neighbouring text) but so the insertion does
// not merge with the current line's text when the cursor was not at its start or
// end.
export function insertTable(view: EditorView) {
  const { state } = view;
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  const needsLeadingBlank = line.text.trim() !== "";
  const table = serializeTable(emptyTable(2, 2));
  const insert = (needsLeadingBlank ? "\n\n" : "\n") + table + "\n\n";
  view.dispatch({
    changes: { from: line.to, insert },
    selection: EditorSelection.cursor(line.to + insert.length),
    scrollIntoView: true,
  });
  view.focus();
}
