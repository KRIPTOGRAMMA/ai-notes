import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { Routine } from "../types";

let routines: Routine[] = $state([]);
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

export const routineStore = {
  get routines() { return routines; },
  get active() { return routines.filter(r => r.active); },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    routines = await guard(() => api.getRoutines(), routines);
  },

  async create(title: string, daysMask: number, startMins: number, durationMins: number) {
    const ok = await guard(async () => {
      await api.createRoutine({ title, days_mask: daysMask, start_mins: startMins, duration_mins: durationMins });
      return true;
    }, false);
    if (ok) await routineStore.load();
  },

  async update(id: string, patch: { title?: string; days_mask?: number; start_mins?: number; duration_mins?: number; active?: boolean }) {
    const ok = await guard(async () => { await api.updateRoutine(id, patch); return true; }, false);
    if (ok) await routineStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteRoutine(id); return true; }, false);
    if (ok) await routineStore.load();
  },
};
