// Зеркало src-tauri/src/core/task.rs::Task.
// Rust-сторона не использует #[serde(rename_all)], поэтому имена полей
// в JSON совпадают с именами полей структуры один в один.

export type TaskStatus = "Todo" | "InProgress" | "Done" | "Archived";
export type Priority = "Low" | "Medium" | "High" | "Critical";
export type Category = "Work" | "Study" | "Home" | "Health" | "Other";
export type RecurrenceUnit = "Minutes" | "Hours" | "Days" | "Weeks";

export type Recurrence =
  | "None"
  | "Hourly"
  | "Daily"
  | "Weekly"
  | { Custom: [number, RecurrenceUnit] };

export interface Task {
  id: string;
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: Priority;
  category: Category;
  deadline: string | null; // RFC3339, приходит как строка через JSON
  tags: string[];
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  recurrence: Recurrence;
  hidden: boolean;
}

export interface CreateTaskPayload {
  title: string;
  description: string | null;
  status: TaskStatus;
  priority: Priority;
  category: Category;
  deadline: string | null;
  tags: string[];
  recurrence: Recurrence;
}

export interface UpdateTaskPayload {
  title?: string;
  description?: string;
  status?: TaskStatus;
  priority?: Priority;
  category?: Category;
  deadline?: string;
  tags?: string[];
  recurrence?: Recurrence;
}

export interface Note {
  id: string;
  title: string;
  content: string;
  created_at: string;
  updated_at: string;
}

export interface CreateNotePayload {
  title: string;
  content: string;
}

export interface UpdateNotePayload {
  title?: string;
  content?: string;
}

export interface AppSettings {
  ai_provider: "local" | "openai" | "anthropic";
  openai_key: string;
  openai_model: string;
  anthropic_key: string;
  anthropic_model: string;
  idle_threshold_secs: number;
  log_interval_secs: number;
}
