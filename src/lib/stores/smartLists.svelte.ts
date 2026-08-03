import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { SmartList, SmartListFilter } from "../types";

let lists: SmartList[] = $state([]);
let error: string | null = $state(null);

// See tasks.svelte.ts for why this wrapper stays local instead of moving into
// guard.ts: it closes over the store's own `error` rune.
async function guard<T>(op: () => Promise<T>, fallback: T): Promise<T> {
  const r = await runGuarded(op);
  if (r.ok) {
    error = null;
    return r.value;
  }
  error = r.error;
  return fallback;
}

export const smartListStore = {
  get lists() { return lists; },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    lists = await guard(() => api.getSmartLists(), lists);
  },

  async create(name: string, filter: SmartListFilter) {
    const ok = await guard(async () => { await api.createSmartList(name, filter); return true; }, false);
    if (ok) await smartListStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteSmartList(id); return true; }, false);
    if (ok) await smartListStore.load();
  },
};
