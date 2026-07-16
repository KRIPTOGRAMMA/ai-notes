import { api } from "../api/tauri";
import type { CategoryInfo } from "../types";

let categories: CategoryInfo[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const categoryStore = {
  get categories() { return categories; },
  get error() { return error; },
  clearError() { error = null; },

  // Отображение по id с фолбэком на сам id (задачи со старой/чужой категорией)
  name(id: string): string {
    return categories.find(c => c.id === id)?.name ?? id;
  },
  color(id: string): string {
    return categories.find(c => c.id === id)?.color ?? "#888888";
  },

  async load() {
    try {
      categories = await api.getCategories();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(name: string, color: string) {
    try {
      await api.createCategory(name, color);
      await categoryStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async update(id: string, patch: { name?: string; color?: string }) {
    try {
      await api.updateCategory(id, patch);
      await categoryStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteCategory(id);
      await categoryStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
