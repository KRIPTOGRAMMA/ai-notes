import { api } from "../api/tauri";
import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

let tasks: Task[] = $state([]);
let error: string | null = $state(null);

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
