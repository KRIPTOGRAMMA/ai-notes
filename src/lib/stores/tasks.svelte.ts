import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

let tasks: Task[] = $state([]);
let deletedTasks: Task[] = $state([]);
let error: string | null = $state(null);
// Сигнал «открыть эту задачу» — ставится из глобального поиска, Tasks.svelte
// реагирует через $effect и открывает TaskModal.
let focusTaskId: string | null = $state(null);
let createRequested = $state(0); // инкремент — сигнал открыть модалку создания
let planDayRequested = $state(0); // инкремент — сигнал перейти в Календарь-неделю и запустить план дня

// Обёртка вместо try/catch в каждом методе (v0.9.25). Главное здесь —
// сброс error на успехе: раньше он только выставлялся, и первая же ошибка
// оставляла баннер висеть навсегда, даже когда всё уже работало.
// Успех/ошибка различаются по флагу ok, а не по truthiness результата —
// иначе методы, законно возвращающие null/0/false, считались бы упавшими.
// fallback возвращается вызывающему при ошибке (null/[] — по сигнатуре).
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
  // Произвольная операция под тем же баннером ошибок, что и остальной стор
  // (v0.9.45): чек-лист в панели строки шлёт свой diff несколькими вызовами
  // api и не укладывается в готовые методы, но ошибку показывать обязан там
  // же, где её ждёт пользователь.
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

  // Возвращает созданную задачу — модалке нужен id, чтобы дописать подзадачи
  // из инлайн-чеклиста (v0.8.3) сразу после создания.
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

  async remove(id: string) {
    if (await guard(async () => { await api.deleteTask(id); return true; }, false)) {
      await taskStore.load();
      // Обновляем и корзину — если панель сейчас открыта, задача должна
      // появиться в ней сразу, а не только при следующем ручном переключении.
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
