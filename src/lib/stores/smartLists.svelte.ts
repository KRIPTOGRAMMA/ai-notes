import { api } from "../api/tauri";
import type { SmartList, SmartListFilter } from "../types";

let lists: SmartList[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const smartListStore = {
  get lists() { return lists; },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    try {
      lists = await api.getSmartLists();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(name: string, filter: SmartListFilter) {
    try {
      await api.createSmartList(name, filter);
      await smartListStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteSmartList(id);
      await smartListStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
