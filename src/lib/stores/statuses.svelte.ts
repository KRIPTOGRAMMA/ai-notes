import { api } from "../api/tauri";
import { seededName } from "../i18n";
import { i18n } from "../i18n.svelte";
import type { StatusInfo } from "../types";

let statuses: StatusInfo[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
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
    try {
      statuses = await api.getStatuses();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(name: string, color: string) {
    try {
      await api.createStatus(name, color);
      await statusStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async update(id: string, patch: { name?: string; color?: string }) {
    try {
      await api.updateStatus(id, patch);
      await statusStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteStatus(id);
      await statusStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
