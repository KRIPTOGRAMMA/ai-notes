// Keyboard navigation over a list of rows (v0.9.77).
//
// Why this is a separate .ts module rather than logic inside the two views:
// vitest has no svelte plugin here, so anything living in a .svelte file is only
// reachable from Playwright. The two parts most likely to break silently — the
// decision to swallow a key at all, and where the cursor lands — are pure
// functions, so they are testable directly.
//
// The views keep only the wiring: reading document.activeElement, calling the
// action, and scrolling the row into view.

/** Actions a list row can receive from the keyboard. */
export type NavAction = "down" | "up" | "open" | "complete" | "delete" | "escape";

/**
 * Whether a keystroke is meant for the list rather than for a field the user is
 * typing in.
 *
 * `el` is document.activeElement (the same question QuickCapture.dictate() asks,
 * v0.9.66). Typing "j" into the composer must produce the letter, not move the
 * cursor — that is the regression this guards.
 *
 * contenteditable is checked too: the CodeMirror editor in Notes is neither an
 * <input> nor a <textarea>, and without this a keystroke in the note body would
 * be stolen by the list.
 */
export function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) {
    return true;
  }
  if (!(el instanceof HTMLElement)) return false;
  // Two checks, not one. isContentEditable is the correct answer in a browser — it
  // is inherited, so it is true for a node *inside* an editable region, which is
  // where the caret actually sits in CodeMirror. But jsdom does not implement it
  // (always undefined), so relying on it alone would leave this branch untestable.
  // closest() covers the same inheritance and works in both.
  if (el.isContentEditable === true) return true;
  return el.closest("[contenteditable]:not([contenteditable='false'])") !== null;
}

/**
 * Maps a keyboard event to a list action, or null when the list should ignore it.
 *
 * Modifiers are rejected outright: Ctrl+J is the global "quick slot" hotkey and a
 * bare `j` is list movement — treating them the same would hijack the hotkeys.
 *
 * Arrows work alongside j/k so the feature is discoverable without knowing vim.
 * `e.key` (not `e.code`) is deliberate for the letters: on a Russian layout the
 * key under "J" produces "о", and the letters here are a convenience — the arrows
 * are the layout-independent path. Global hotkeys use e.code for the opposite
 * reason (v0.9.66), because there the binding must survive the layout.
 */
export function actionForKey(e: {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  defaultPrevented?: boolean;
}): NavAction | null {
  // A second line of defence behind isTypingTarget. Overlays sit on top of a view
  // that stays mounted, and both listeners hang off <svelte:window> — the same node
  // — so bubbling never separates them and listener order decides who runs first.
  // The search palette holds focus in its own input, so isTypingTarget already stops
  // it; a modal with no field (the history card) does not, and marking the event is
  // what keeps its Escape from also clearing the cursor underneath.
  if (e.defaultPrevented) return null;
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  switch (e.key) {
    case "j":
    case "ArrowDown": return "down";
    case "k":
    case "ArrowUp": return "up";
    case "Enter": return "open";
    case " ": return "complete";
    case "Delete": return "delete";
    case "Escape": return "escape";
    default: return null;
  }
}

/**
 * The cursor's next position.
 *
 * `current` is -1 when nothing is focused yet, and the first keystroke in either
 * direction must land on a row rather than being swallowed: `j` on an untouched
 * list goes to the first row, `k` to the last.
 *
 * The ends do not wrap. Wrapping would mean that holding `j` silently jumps back
 * to the top of a long list, which reads as a glitch rather than as an end stop.
 * An empty list keeps the cursor at -1.
 */
export function nextIndex(current: number, delta: number, length: number): number {
  if (length === 0) return -1;
  if (current < 0) return delta > 0 ? 0 : length - 1;
  const next = current + delta;
  if (next < 0) return 0;
  if (next >= length) return length - 1;
  return next;
}

/**
 * Keeps the cursor pointing at the same row when the list changes underneath it.
 *
 * The list is re-sorted or refiltered on every save, and a cursor stored as a bare
 * index would quietly drift onto a different task. Completing a row removes it, so
 * the id disappears entirely: the cursor then stays on the same position, which is
 * now the following row — the natural place to keep working from.
 */
export function reconcileIndex(prevId: string | null, ids: string[], prevIndex: number): number {
  if (prevId === null || prevIndex < 0) return -1;
  const found = ids.indexOf(prevId);
  if (found >= 0) return found;
  if (ids.length === 0) return -1;
  return Math.min(prevIndex, ids.length - 1);
}
