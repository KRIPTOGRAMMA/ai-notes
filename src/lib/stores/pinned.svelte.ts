import { api } from "../api/tauri";
import type { PinnedItem } from "../types";
import { runGuarded } from "../guard";

// The "quick slot": a single pinned task or note under a global hotkey. The store
// is needed by both views at once: pinning is possible from Tasks and from Notes
// while there is only one slot, so pinning a task must clear the highlight from a
// previously pinned note, and vice versa.
let item: PinnedItem | null = $state(null);
let error: string | null = $state(null);

export const pinnedStore = {
  get item() { return item; },
  get error() { return error; },
  clearError() { error = null; },

  // Whether this particular item is pinned. Both the kind and the id are checked:
  // ids are generated independently for tasks and notes, so the id alone is not
  // enough.
  is(kind: "task" | "note", id: string): boolean {
    return item?.kind === kind && item.id === id;
  },

  async load() {
    const r = await runGuarded(() => api.getPinnedItem());
    if (r.ok) { item = r.value; error = null; }
    else error = r.error;
  },

  // Pressing again on a pinned item unpins it: one button instead of two.
  async toggle(kind: "task" | "note", id: string) {
    const unpin = pinnedStore.is(kind, id);
    const r = await runGuarded(() =>
      api.setPinnedItem(unpin ? null : kind, unpin ? null : id)
    );
    if (!r.ok) { error = r.error; return; }
    error = null;
    await pinnedStore.load();
  },
};
