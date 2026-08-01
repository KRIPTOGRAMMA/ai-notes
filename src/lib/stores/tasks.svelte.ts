import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

let tasks: Task[] = $state([]);
let deletedTasks: Task[] = $state([]);
let error: string | null = $state(null);
// The "open this task" signal, set from global search; Tasks.svelte reacts through
// an $effect and opens TaskModal.
let focusTaskId: string | null = $state(null);
let createRequested = $state(0); // an increment signals opening the creation modal
let planDayRequested = $state(0); // an increment signals switching to the Calendar week and running the day plan

// A wrapper instead of a try/catch in every method. The key part is clearing error
// on success: it used to be only set, so the very first failure left the banner
// hanging forever, even once everything worked again. Success and failure are told
// apart by the ok flag rather than by the result's truthiness — otherwise methods
// that legitimately return null, 0 or false would count as failed. On an error the
// fallback is returned to the caller (null or [], per the signature).
async function guard<T>(op: () => Promise<T>, fallback: T): Promise<T> {
  const r = await runGuarded(op);
  if (r.ok) {
    error = null;
    return r.value;
  }
  error = r.error;
  return fallback;
}

export const taskStore = {
  get tasks() { return tasks; },
  get activeTasks() { return tasks.filter(t => !t.hidden); },
  get historyTasks() { return tasks.filter(t => t.hidden); },
  get deletedTasks() { return deletedTasks; },
  get error() { return error; },
  clearError() { error = null; },
  // An arbitrary operation under the same error banner as the rest of the store: the
  // checklist in a row's panel sends its diff through several api calls and does not
  // fit the ready-made methods, yet it must surface an error in the same place the
  // user expects one.
  async guarded(op: () => Promise<void>): Promise<void> {
    await guard(async () => { await op(); }, undefined);
  },
  get focusTaskId() { return focusTaskId; },
  requestFocus(id: string) { focusTaskId = id; },
  clearFocus() { focusTaskId = null; },
  get createRequested() { return createRequested; },
  requestCreate() { createRequested++; },
  get planDayRequested() { return planDayRequested; },
  requestPlanDay() { planDayRequested++; },

  async load() {
    tasks = await guard(() => api.getTasks(), tasks);
  },

  // Returns the created task: the modal needs the id to append the subtasks from the
  // inline checklist right after creation.
  async create(payload: CreateTaskPayload): Promise<Task | null> {
    const task = await guard(() => api.createTask(payload), null);
    if (task) await taskStore.load();
    return task;
  },

  async update(id: string, patch: UpdateTaskPayload) {
    if (await guard(async () => { await api.updateTask(id, patch); return true; }, false)) {
      await taskStore.load();
    }
  },

  async complete(id: string) {
    if (await guard(async () => { await api.completeTask(id); return true; }, false)) {
      await taskStore.load();
    }
  },

  // Dependencies. Both operations change a task's blocked_by, so the list is
  // reloaded afterwards: the dimming and the completion ban are computed by the
  // backend and cannot be reproduced locally.
  async addDependency(taskId: string, blockerId: string) {
    if (await guard(async () => { await api.addTaskDependency(taskId, blockerId); return true; }, false)) {
      await taskStore.load();
    }
  },

  async removeDependency(taskId: string, blockerId: string) {
    if (await guard(async () => { await api.removeTaskDependency(taskId, blockerId); return true; }, false)) {
      await taskStore.load();
    }
  },

  async remove(id: string) {
    if (await guard(async () => { await api.deleteTask(id); return true; }, false)) {
      await taskStore.load();
      // The Trash is refreshed too: if that panel is open right now the task must
      // appear in it at once rather than only on the next manual switch.
      await taskStore.loadDeleted();
    }
  },

  async loadDeleted() {
    deletedTasks = await guard(() => api.getDeletedTasks(), deletedTasks);
  },

  async restore(id: string) {
    if (await guard(async () => { await api.restoreTask(id); return true; }, false)) {
      await taskStore.loadDeleted();
      await taskStore.load();
    }
  },

  async purge(id: string) {
    if (await guard(async () => { await api.purgeDeletedTask(id); return true; }, false)) {
      await taskStore.loadDeleted();
    }
  },

  async reorder(ids: string[]) {
    if (await guard(async () => { await api.reorderTasks(ids); return true; }, false)) {
      await taskStore.load();
    }
  },

  async search(query: string): Promise<Task[]> {
    if (!query.trim()) return [];
    return await guard(() => api.searchTasks(query), []);
  },
};
