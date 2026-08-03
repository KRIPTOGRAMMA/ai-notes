import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import { seededName } from "../i18n";
import { i18n } from "../i18n.svelte";
import type { CategoryInfo } from "../types";

let categories: CategoryInfo[] = $state([]);
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

export const categoryStore = {
  get categories() { return categories; },
  get error() { return error; },
  clearError() { error = null; },

  // Display by id, falling back to the id itself (tasks carrying an old or foreign
  // category). Seeded categories are translated — we wrote their names in a
  // migration and they are as much part of the interface as the labels on buttons.
  // User-defined and renamed ones are returned as is; seededName decides by id.
  name(id: string): string {
    const c = categories.find(c => c.id === id);
    if (!c) return id;
    return seededName("category", c.id, c.name, i18n.lang);
  },
  color(id: string): string {
    return categories.find(c => c.id === id)?.color ?? "#888888";
  },

  async load() {
    categories = await guard(() => api.getCategories(), categories);
  },

  async create(name: string, color: string) {
    const ok = await guard(async () => { await api.createCategory(name, color); return true; }, false);
    if (ok) await categoryStore.load();
  },

  async update(id: string, patch: { name?: string; color?: string }) {
    const ok = await guard(async () => { await api.updateCategory(id, patch); return true; }, false);
    if (ok) await categoryStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteCategory(id); return true; }, false);
    if (ok) await categoryStore.load();
  },
};
