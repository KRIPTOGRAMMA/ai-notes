import { api } from "../api/tauri";
import type { Routine } from "../types";

let routines: Routine[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

export const routineStore = {
  get routines() { return routines; },
  get active() { return routines.filter(r => r.active); },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    try {
      routines = await api.getRoutines();
      error = null;
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(title: string, daysMask: number, startMins: number, durationMins: number) {
    try {
      await api.createRoutine({ title, days_mask: daysMask, start_mins: startMins, duration_mins: durationMins });
      await this.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async update(id: string, patch: { title?: string; days_mask?: number; start_mins?: number; duration_mins?: number; active?: boolean }) {
    try {
      await api.updateRoutine(id, patch);
      await this.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteRoutine(id);
      await this.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
