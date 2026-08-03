import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { Project, UpdateProjectPayload } from "../types";

let projects: Project[] = $state([]);
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

export const projectStore = {
  get projects() { return projects; },
  get active() { return projects.filter(p => !p.archived); },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    projects = await guard(() => api.getProjects(), projects);
  },

  async create(name: string, color = ""): Promise<Project | null> {
    const p = await guard(() => api.createProject({ name, color }), null);
    if (p) await projectStore.load();
    return p;
  },

  async update(id: string, patch: UpdateProjectPayload) {
    const ok = await guard(async () => { await api.updateProject(id, patch); return true; }, false);
    if (ok) await projectStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteProject(id); return true; }, false);
    if (ok) await projectStore.load();
  },
};
