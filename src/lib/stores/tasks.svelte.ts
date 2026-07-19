import { api } from "../api/tauri";
import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

let tasks: Task[] = $state([]);
let error: string | null = $state(null);
// Сигнал «открыть эту задачу» — ставится из глобального поиска, Tasks.svelte
// реагирует через $effect и открывает TaskModal.
let focusTaskId: string | null = $state(null);
let createRequested = $state(0); // инкремент — сигнал открыть модалку создания
let planDayRequested = $state(0); // инкремент — сигнал перейти в Календарь-неделю и запустить план дня

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const taskStore = {
  get tasks() { return tasks; },
  get activeTasks() { return tasks.filter(t => !t.hidden); },
  get historyTasks() { return tasks.filter(t => t.hidden); },
  get error() { return error; },
  clearError() { error = null; },
  get focusTaskId() { return focusTaskId; },
  requestFocus(id: string) { focusTaskId = id; },
  clearFocus() { focusTaskId = null; },
  get createRequested() { return createRequested; },
  requestCreate() { createRequested++; },
  get planDayRequested() { return planDayRequested; },
  requestPlanDay() { planDayRequested++; },

  async load() {
    try {
      tasks = await api.getTasks();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(payload: CreateTaskPayload) {
    try {
      await api.createTask(payload);
      await taskStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async update(id: string, patch: UpdateTaskPayload) {
    try {
      await api.updateTask(id, patch);
      await taskStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async complete(id: string) {
    try {
      await api.completeTask(id);
      await taskStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteTask(id);
      await taskStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async reorder(ids: string[]) {
    try {
      await api.reorderTasks(ids);
      await taskStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async search(query: string): Promise<Task[]> {
    if (!query.trim()) return [];
    try {
      return await api.searchTasks(query);
    } catch (e) {
      error = describeError(e);
      return [];
    }
  },
};
