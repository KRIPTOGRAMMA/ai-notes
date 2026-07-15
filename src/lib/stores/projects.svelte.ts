import { api } from "../api/tauri";
import type { Project, UpdateProjectPayload } from "../types";

let projects: Project[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const projectStore = {
  get projects() { return projects; },
  get active() { return projects.filter(p => !p.archived); },
  get error() { return error; },
  clearError() { error = null; },

  async load() {
    try {
      projects = await api.getProjects();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(name: string, color = ""): Promise<Project | null> {
    try {
      const p = await api.createProject({ name, color });
      await projectStore.load();
      return p;
    } catch (e) {
      error = describeError(e);
      return null;
    }
  },

  async update(id: string, patch: UpdateProjectPayload) {
    try {
      await api.updateProject(id, patch);
      await projectStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteProject(id);
      await projectStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
