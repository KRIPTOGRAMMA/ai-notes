import { invoke } from "@tauri-apps/api/core";
import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

export const api = {
  getTasks: () => invoke<Task[]>("get_tasks"),
  createTask: (task: CreateTaskPayload) => invoke<Task>("create_task", { task }),
  updateTask: (id: string, patch: UpdateTaskPayload) => invoke<Task>("update_task", { id, patch }),
  deleteTask: (id: string) => invoke<void>("delete_task", { id }),
  completeTask: (id: string) => invoke<Task>("complete_task", { id }),
  searchTasks: (query: string) => invoke<Task[]>("search_tasks", { query }),
  recordInput: () => invoke<void>("record_input"),
  openQuickTask: () => invoke<void>("open_quick_task"),
  aiRewrite: (taskId: string, title: string) => invoke<void>("ai_rewrite", { taskId, title }),
  aiSubtasks: (taskId: string, title: string) => invoke<void>("ai_subtasks", { taskId, title }),
  aiClassify: (taskId: string, title: string) => invoke<void>("ai_classify", { taskId, title }),
};
