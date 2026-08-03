import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import { seededName } from "../i18n";
import { i18n } from "../i18n.svelte";
import type { StatusInfo } from "../types";

let statuses: StatusInfo[] = $state([]);
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

export const statusStore = {
  get statuses() { return statuses; },
  get error() { return error; },
  clearError() { error = null; },

  // Display by id, falling back to the id itself (tasks carrying an old or foreign
  // status). Seeded ones (is_reserved) are translated — see categories.svelte.ts
  name(id: string): string {
    const s = statuses.find(s => s.id === id);
    if (!s) return id;
    return seededName("status", s.id, s.name, i18n.lang);
  },
  color(id: string): string {
    return statuses.find(s => s.id === id)?.color ?? "#888888";
  },
  isReserved(id: string): boolean {
    return statuses.find(s => s.id === id)?.is_reserved ?? false;
  },

  async load() {
    statuses = await guard(() => api.getStatuses(), statuses);
  },

  async create(name: string, color: string) {
    const ok = await guard(async () => { await api.createStatus(name, color); return true; }, false);
    if (ok) await statusStore.load();
  },

  async update(id: string, patch: { name?: string; color?: string }) {
    const ok = await guard(async () => { await api.updateStatus(id, patch); return true; }, false);
    if (ok) await statusStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteStatus(id); return true; }, false);
    if (ok) await statusStore.load();
  },
};
