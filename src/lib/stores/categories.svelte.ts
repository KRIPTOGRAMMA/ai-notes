import { api } from "../api/tauri";
import { seededName } from "../i18n";
import { i18n } from "../i18n.svelte";
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

  // Отображение по id с фолбэком на сам id (задачи со старой/чужой категорией).
  // Посевные категории переводятся (v0.9.47) — их имена написали мы миграцией,
  // и это такая же часть интерфейса, как надписи на кнопках. Пользовательские
  // и переименованные отдаются как есть; решает seededName по id.
  name(id: string): string {
    const c = categories.find(c => c.id === id);
    if (!c) return id;
    return seededName("category", c.id, c.name, i18n.lang);
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
