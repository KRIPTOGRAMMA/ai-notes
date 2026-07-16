// Мок Tauri-бэкенда для E2E: подсовывается через page.addInitScript ДО кода
// приложения. @tauri-apps/api v2 весь ходит через window.__TAURI_INTERNALS__
// (invoke + transformCallback) — реализуем оба, и фронт работает как в Tauri.
//
// БД мока сериализуется в localStorage (__mock_db) на каждой мутации, поэтому
// переживает page.reload(). Тест может сидировать состояние, положив свой
// __mock_db в localStorage init-скриптом, добавленным ПЕРЕД этим файлом.
(() => {
  const now = () => new Date().toISOString();
  const uuid = () =>
    (crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2));

  const defaultSettings = {
    ai_provider: "none",
    openai_key: "",
    openai_model: "gpt-4o-mini",
    anthropic_key: "",
    anthropic_model: "claude-haiku-4-5-20251001",
    idle_threshold_secs: 300,
    log_interval_secs: 60,
    work_mode: "Light",
    onboarding_complete: true,
    deadline_warn_hours: 24,
    deadline_warn_minutes: 60,
    idle_notify_min_mins: 10,
    pomodoro_work_mins: 25,
    pomodoro_break_mins: 5,
    nudge_after_mins: 90,
    theme_mode: "system",
    color_accent: "",
    color_bg: "",
    color_text: "",
    color_border: "",
    quiet_until: "",
    context_notifications: true,
    ai_fallback: false,
    openai_in_keyring: false,
    anthropic_in_keyring: false,
    app_category_rules: "",
  };

  let db;
  try {
    db = JSON.parse(localStorage.getItem("__mock_db") ?? "null");
  } catch {
    db = null;
  }
  if (!db) db = { tasks: [], notes: [], projects: [], settings: { ...defaultSettings } };
  if (!db.projects) db.projects = [];
  for (const p of db.projects) {
    p.goal_period ??= "week";
    p.goal_tasks ??= null;
    p.goal_mins ??= null;
  }
  // сид может задать только часть настроек
  db.settings = { ...defaultSettings, ...db.settings };

  const persist = () => localStorage.setItem("__mock_db", JSON.stringify(db));
  persist();

  // Реестр слушателей событий (plugin:event|listen). Колбэки живут в
  // __mockCallbacks под id, выданным transformCallback.
  const callbacks = new Map();
  let nextCallbackId = 1;
  const eventHandlers = new Map(); // event -> Set<callbackId>

  window.__mockEmit = (event, payload) => {
    for (const id of eventHandlers.get(event) ?? []) {
      const cb = callbacks.get(id);
      if (cb) cb({ event, payload, id });
    }
  };

  window.__unknownInvokes = [];

  const findTask = (id) => db.tasks.find((t) => t.id === id);

  const commands = {
    // --- настройки / окружение ---
    get_settings: () => ({ ...db.settings }),
    save_settings: ({ settings }) => {
      db.settings = { ...db.settings, ...settings };
      persist();
    },
    is_wayland: () => false,
    get_tracking_mode: () => "basic",
    get_window_tracking: () => null,
    record_input: () => {},
    open_quick_capture: ({ mode }) => {
      db.quickMode = mode;
      persist();
    },
    get_quick_mode: () => db.quickMode ?? "task",

    // --- задачи ---
    get_tasks: () => db.tasks.map((t) => ({ ...t })),
    create_task: ({ task }) => {
      const full = {
        id: uuid(),
        description: null,
        deadline: null,
        tags: [],
        completed_at: null,
        recurrence: "None",
        hidden: false,
        project_id: null,
        scheduled_at: null,
        scheduled_mins: null,
        subtasks: [],
        ...task,
        created_at: now(),
        updated_at: now(),
      };
      db.tasks.push(full);
      persist();
      return { ...full };
    },
    update_task: ({ id, patch }) => {
      const t = findTask(id);
      if (!t) throw `Задача не найдена: ${id}`;
      for (const [k, v] of Object.entries(patch)) {
        if (v === undefined) continue;
        // конвенции бэкенда: пустая строка = снять значение
        if (k === "project_id") t.project_id = v === "" ? null : v;
        else if (k === "scheduled_at") {
          if (v === "") { t.scheduled_at = null; t.scheduled_mins = null; }
          else t.scheduled_at = v;
        } else t[k] = v;
      }
      t.updated_at = now();
      persist();
      return { ...t };
    },
    delete_task: ({ id }) => {
      db.tasks = db.tasks.filter((t) => t.id !== id);
      persist();
    },
    complete_task: ({ id }) => {
      const t = findTask(id);
      if (!t) throw `Задача не найдена: ${id}`;
      // как в Rust (recurrence None): Done + hidden — задача уходит в историю
      t.status = "Done";
      t.hidden = true;
      t.completed_at = now();
      t.updated_at = now();
      persist();
      return { ...t };
    },
    search_tasks: ({ query }) => {
      const q = query.toLowerCase();
      return db.tasks.filter((t) => t.title.toLowerCase().includes(q));
    },

    // --- проекты ---
    get_projects: () =>
      db.projects.map((p) => ({
        ...p,
        task_total: db.tasks.filter((t) => t.project_id === p.id).length,
        task_done: db.tasks.filter((t) => t.project_id === p.id && t.completed_at).length,
        // упрощение мока: весь прогресс считаем «в текущем периоде»
        goal_done_tasks: db.tasks.filter((t) => t.project_id === p.id && t.completed_at).length,
        goal_done_mins: db.tasks
          .filter((t) => t.project_id === p.id && t.scheduled_at && new Date(t.scheduled_at) <= new Date())
          .reduce((s, t) => s + (t.scheduled_mins ?? 60), 0),
      })),
    create_project: ({ project }) => {
      const full = {
        id: uuid(),
        color: "",
        target_date: null,
        archived: false,
        goal_tasks: null,
        goal_mins: null,
        goal_period: "week",
        ...project,
        created_at: now(),
      };
      db.projects.push(full);
      persist();
      return { ...full, task_total: 0, task_done: 0, goal_done_tasks: 0, goal_done_mins: 0 };
    },
    update_project: ({ id, patch }) => {
      const p = db.projects.find((p) => p.id === id);
      if (!p) throw `Проект не найден: ${id}`;
      for (const [k, v] of Object.entries(patch)) {
        if (v === undefined) continue;
        // конвенции бэкенда: пустая дата и цель <= 0 = снять
        if (k === "target_date") p.target_date = v === "" ? null : v;
        else if (k === "goal_tasks" || k === "goal_mins") p[k] = v > 0 ? v : null;
        else p[k] = v;
      }
      persist();
    },
    delete_project: ({ id }) => {
      db.projects = db.projects.filter((p) => p.id !== id);
      for (const t of db.tasks) if (t.project_id === id) t.project_id = null;
      persist();
    },

    // --- подзадачи ---
    get_subtasks: ({ taskId }) => findTask(taskId)?.subtasks ?? [],
    add_subtask: ({ taskId, title }) => {
      const t = findTask(taskId);
      if (!t) throw `Задача не найдена: ${taskId}`;
      const sub = { id: uuid(), task_id: taskId, title, done: false, position: t.subtasks.length };
      t.subtasks.push(sub);
      persist();
      return { ...sub };
    },
    toggle_subtask: ({ id }) => {
      for (const t of db.tasks) {
        const s = t.subtasks.find((s) => s.id === id);
        if (s) { s.done = !s.done; persist(); return; }
      }
    },
    delete_subtask: ({ id }) => {
      for (const t of db.tasks) t.subtasks = t.subtasks.filter((s) => s.id !== id);
      persist();
    },

    // --- заметки ---
    get_notes: () => db.notes.map((n) => ({ ...n })),
    create_note: ({ note }) => {
      const full = {
        id: uuid(),
        tags: [],
        linked_task_id: null,
        project_id: null,
        ...note,
        created_at: now(),
        updated_at: now(),
      };
      db.notes.push(full);
      persist();
      return { ...full };
    },
    update_note: ({ id, patch }) => {
      const n = db.notes.find((n) => n.id === id);
      if (!n) throw `Заметка не найдена: ${id}`;
      for (const [k, v] of Object.entries(patch)) {
        if (v !== undefined) n[k] = v;
      }
      n.updated_at = now();
      persist();
      return { ...n };
    },
    delete_note: ({ id }) => {
      db.notes = db.notes.filter((n) => n.id !== id);
      persist();
    },
    search_notes: ({ query }) => {
      const q = (query ?? "").trim().toLowerCase();
      if (!q) return [];
      return db.notes
        .filter((n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q))
        .map((n) => ({ ...n }));
    },

    // --- дашборд / ИИ / модель ---
    get_activity_by_day: () => [],
    get_task_completions_by_day: () => [],
    get_category_distribution: () => [],
    get_active_idle_ratio: () => ({ today_active: 0, today_idle: 0, week_active: 0, week_idle: 0 }),
    get_app_usage: () => [],
    get_app_category_time: () => [],
    dashboard_insight: () => {},
    summarize_day: () => {},
    summarize_week: () => {},
    // Планировщик: детерминированный «ИИ» — первый бэклог-таск в 10:00 на 60 мин
    ai_plan_day: () => {
      const t = db.tasks.find(
        (t) => !t.hidden && !t.scheduled_at && (t.status === "Todo" || t.status === "InProgress"),
      );
      const at = new Date();
      at.setHours(10, 0, 0, 0);
      setTimeout(() => window.__mockEmit("ai-plan", {
        blocks: t ? [{ id: t.id, title: t.title, scheduled_at: at.toISOString(), mins: 60 }] : [],
        error: t ? null : "Бэклог пуст — нечего планировать",
      }), 0);
    },
    ai_what_now: () => {
      setTimeout(() => window.__mockEmit("ai-what-now", {
        result: "Совет мока: начните с самой приоритетной задачи.",
        error: null,
      }), 0);
    },
    ai_rewrite: () => {},
    ai_subtasks: () => {},
    ai_classify: () => {},
    model_status: () => ({ exists: false, size_bytes: 0 }),
    default_model_url: () => "",
    export: () => {},
    import: () => {},

    // --- плагины ---
    "plugin:event|listen": ({ event, handler }) => {
      if (!eventHandlers.has(event)) eventHandlers.set(event, new Set());
      eventHandlers.get(event).add(handler);
      return handler;
    },
    "plugin:event|unlisten": ({ event, eventId }) => {
      eventHandlers.get(event)?.delete(eventId);
    },
    "plugin:autostart|enable": () => {},
    "plugin:autostart|disable": () => {},
    "plugin:autostart|is_enabled": () => false,
    "plugin:dialog|save": () => null,
    "plugin:dialog|open": () => null,
  };

  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) {
      const id = nextCallbackId++;
      callbacks.set(id, cb);
      return id;
    },
    async invoke(cmd, args = {}) {
      const handler = commands[cmd];
      if (!handler) {
        window.__unknownInvokes.push(cmd);
        return undefined;
      }
      return handler(args);
    },
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main" },
    },
  };
})();
