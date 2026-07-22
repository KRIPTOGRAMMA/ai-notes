// Зеркало src-tauri/src/core/task.rs::Task.
// Rust-сторона не использует #[serde(rename_all)], поэтому имена полей
// в JSON совпадают с именами полей структуры один в один.

export type TaskStatus = "Todo" | "InProgress" | "Done" | "Archived";
export type Priority = "Low" | "Medium" | "High" | "Critical";
// С v0.6.3 категория — id строки в таблице categories (пользовательские),
// а не фиксированный набор. Имя/цвет — через CategoryInfo.
export type Category = string;
export type RecurrenceUnit = "Minutes" | "Hours" | "Days" | "Weeks";

export interface CategoryInfo {
  id: string;
  name: string;
  color: string;
  position: number;
}

export type Recurrence =
  | "None"
  | "Hourly"
  | "Daily"
  | "Weekly"
  | { Custom: [number, RecurrenceUnit] }
  | { Weekdays: number }; // битовая маска: бит 0 = Пн ... бит 6 = Вс

export interface Subtask {
  id: string;
  task_id: string;
  title: string;
  done: boolean;
  position: number;
}

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
  deleted_at: string | null; // мягкое удаление (v0.8.12) — не null = в корзине
  project_id: string | null;
  scheduled_at: string | null; // тайм-блок: начало (RFC3339)
  scheduled_mins: number | null; // тайм-блок: длительность
  sort_order: number; // ручной порядок в списке (drag)
  subtasks: Subtask[];
}

export interface Project {
  id: string;
  name: string;
  color: string;
  target_date: string | null;
  archived: boolean;
  created_at: string;
  task_total: number;
  task_done: number;
  goal_tasks: number | null; // цель: задач за период
  goal_mins: number | null; // цель: минут тайм-блоков за период
  goal_period: "week" | "month";
  goal_done_tasks: number; // прогресс за текущий период
  goal_done_mins: number;
}

export interface GoalSnapshot {
  id: string;
  project_id: string;
  period_key: string;
  goal_tasks: number | null;
  goal_mins: number | null;
  done_tasks: number;
  done_mins: number;
  recorded_at: string;
}

export interface UpdateProjectPayload {
  name?: string;
  color?: string;
  target_date?: string; // пустая строка = убрать дату
  archived?: boolean;
  goal_tasks?: number; // 0 = снять цель
  goal_mins?: number; // 0 = снять цель
  goal_period?: "week" | "month";
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
  project_id?: string | null;
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
  project_id?: string; // пустая строка = отвязать от проекта
  scheduled_at?: string; // пустая строка = снять тайм-блок
  scheduled_mins?: number;
}

export interface Note {
  id: string;
  title: string;
  content: string;
  tags: string[];
  linked_task_id: string | null;
  project_id: string | null;
  pinned: boolean;
  created_at: string;
  updated_at: string;
}

export interface NoteSnippet {
  item: Note;
  snippet: string;
}

export interface NoteRevision {
  id: string;
  created_at: string;
  size: number;
}

export interface TaskSnippet {
  item: Task;
  snippet: string;
}

export interface CreateNotePayload {
  title: string;
  content: string;
  tags?: string[];
  linked_task_id?: string | null;
  project_id?: string | null;
}

export interface UpdateNotePayload {
  title?: string;
  content?: string;
  tags?: string[];
  linked_task_id?: string | null;
  project_id?: string | null;
  pinned?: boolean;
}

export interface AppSettings {
  ai_provider: "none" | "local" | "openai" | "anthropic";
  openai_key: string;
  openai_model: string;
  anthropic_key: string;
  anthropic_model: string;
  idle_threshold_secs: number;
  log_interval_secs: number;
  work_mode: "Light" | "Study" | "Focus";
  onboarding_complete: boolean;
  deadline_warn_hours: number;
  deadline_warn_minutes: number;
  idle_notify_min_mins: number;
  pomodoro_work_mins: number;
  pomodoro_break_mins: number;
  nudge_after_mins: number;
  theme_mode: "light" | "dark" | "system";
  color_accent: string;
  color_accent_secondary: string; // второй акцент (градиент на .btn-primary); пусто = равен color_accent
  color_bg: string;
  color_text: string;
  color_border: string;
  quiet_until: string; // RFC3339; пусто = выкл; далёкая дата = бессрочно
  context_notifications: boolean;
  ai_fallback: boolean;
  openai_in_keyring: boolean;
  anthropic_in_keyring: boolean;
  app_category_rules: string; // JSON [{pattern, category}]
  app_limits: string;         // JSON [{category, daily_mins}] — 0/отсутствие = без лимита
  auto_backup_dir: string;    // пусто = авто-бэкап выключен
  auto_backup_keep: number;   // сколько копий хранить (мин 1)
  morning_digest_time: string; // "HH:MM", пусто = выкл
  show_subtasks_expanded: boolean; // подзадачи в списке видны без клика (v0.8.3)
  keybinds: string; // JSON {action_id: combo} (v0.8.9); отсутствие ключа = дефолт действия
}

export interface AppCategoryRule {
  pattern: string;
  category: Category;
}

export interface AppLimit {
  category: Category;
  daily_mins: number;
}

export interface ChecklistTemplate {
  id: string;
  name: string;
  items: string[];
}

export interface DayCompletion {
  id: string;
  title: string;
}

export interface Routine {
  id: string;
  title: string;
  days_mask: number;
  start_mins: number;
  duration_mins: number;
  active: boolean;
}

export interface RoutineBlock {
  title: string;
  start_mins: number;
  duration_mins: number;
}

export interface ActiveSession {
  task_id: string;
  title: string;
  started_at: string;
  elapsed_secs: number;
}

export interface ModelOption {
  id: string;
  name: string;
  url: string;
  size_bytes: number;
  description: string;
  ram_gb: number;
  recommended: boolean;
}
