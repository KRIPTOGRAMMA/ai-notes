<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import { statusStore } from "../lib/stores/statuses.svelte";
  import { smartListStore } from "../lib/stores/smartLists.svelte";
  import { pinnedStore } from "../lib/stores/pinned.svelte";
  import { api } from "../lib/api/tauri";
  import { parseComposer, parseTaskText, matchCategoryQuery, SUBTASK_PREFIX } from "../lib/composer";
  import { t } from "../lib/i18n.svelte";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import ChecklistEditor from "../lib/components/ChecklistEditor.svelte";
  import { parseChecklist, formatChecklist } from "../lib/checklistText";
  import TaskHistoryDetail from "../lib/components/TaskHistoryDetail.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import type { Task, Subtask, Category, CreateTaskPayload, UpdateTaskPayload, Project, GoalSnapshot, ActiveSession, SmartListFilter } from "../lib/types";

  type AiResult = { task_id: string; type: string; result?: string; error?: string };

  let showGoalHistory = $state<Record<string, GoalSnapshot[]>>({});
  let goalHistoryLoading = $state<Record<string, boolean>>({});

  // List/History/Trash is a single mutually exclusive switch. These used to be two
  // independent toggles, so both could be open at once as two nearly
  // indistinguishable blocks under the shared list.
  let listSubView = $state<"active" | "history" | "trash">("active");
  let showCreateModal = $state(false);
  let editingTask: Task | null = $state(null);
  let historyDetailTask: Task | null = $state(null);

  // List/Board is a switch in the page head. This used to be a separate
  // Kanban.svelte page and was merged here so the project filter, smart lists and
  // multi-select are shared by both view modes.
  let viewMode = $state<"list" | "board">("list");

  // Projects: the list filter ("all" | "none" | id) and the management modal
  let projectFilter = $state<string>("all");
  let showProjects = $state(false);
  let newProjectName = $state("");

  // Smart lists: the modal for creating one of your own
  let showSmartListModal = $state(false);
  let newSmartListName = $state("");
  let newSmartListCategory = $state("");
  let newSmartListPriority = $state("");
  let newSmartListTag = $state("");
  let newSmartListHasDeadline = $state<"" | "yes" | "no">("");

  function resetSmartListForm() {
    newSmartListName = "";
    newSmartListCategory = "";
    newSmartListPriority = "";
    newSmartListTag = "";
    newSmartListHasDeadline = "";
  }

  async function createSmartList() {
    const filter: SmartListFilter = {
      category: newSmartListCategory || null,
      priority: newSmartListPriority || null,
      tag: newSmartListTag.trim() || null,
      has_deadline: newSmartListHasDeadline === "" ? null : newSmartListHasDeadline === "yes",
    };
    await smartListStore.create(newSmartListName, filter);
    if (!smartListStore.error) {
      showSmartListModal = false;
      resetSmartListForm();
    }
  }

  async function removeSmartList(id: string) {
    if (activeSmartListId === id) activeSmartListId = null;
    await smartListStore.remove(id);
  }

  onMount(() => {
    projectStore.load();
    categoryStore.load();
    statusStore.load();
    smartListStore.load();
    pinnedStore.load();
    // Capability detection: with AI turned off the "What now?" button is simply hidden
    api.getSettings().then(s => {
      aiEnabled = s.ai_provider !== "none";
      autoExpandSubs = s.show_subtasks_expanded;
    }).catch(() => {});
  });

  let aiEnabled = $state(false);
  // Tasks with subtasks are expanded by default (the "Appearance" setting)
  let autoExpandSubs = $state(true);

  // Smart lists: the built-in ones ("Overdue"/"This week") depend on the current
  // date, so they live entirely on the frontend and are not stored in the DB;
  // user-defined ones come from smartListStore, with a predicate over category,
  // priority, tag and whether a deadline is set.
  type BuiltinSmartList = { id: string; name: string; test: (t: Task) => boolean };
  const BUILTIN_SMART_LISTS: BuiltinSmartList[] = $derived([
    {
      id: "__overdue",
      name: t("Просроченные"),
      test: (t) => !!t.deadline && new Date(t.deadline).getTime() < Date.now(),
    },
    {
      id: "__this_week",
      name: t("На этой неделе"),
      test: (t) => {
        if (!t.deadline) return false;
        const d = new Date(t.deadline).getTime();
        const now = Date.now();
        return d >= now && d <= now + 7 * 864e5;
      },
    },
  ]);

  let activeSmartListId: string | null = $state(null);

  function matchesSmartFilter(t: Task, f: SmartListFilter): boolean {
    if (f.category && t.category !== f.category) return false;
    if (f.priority && t.priority !== f.priority) return false;
    if (f.tag && !t.tags.includes(f.tag)) return false;
    if (f.has_deadline === true && !t.deadline) return false;
    if (f.has_deadline === false && t.deadline) return false;
    return true;
  }

  const activeSmartListTest = $derived.by((): ((t: Task) => boolean) | null => {
    if (!activeSmartListId) return null;
    const builtin = BUILTIN_SMART_LISTS.find(l => l.id === activeSmartListId);
    if (builtin) return builtin.test;
    const custom = smartListStore.lists.find(l => l.id === activeSmartListId);
    if (custom) return (t: Task) => matchesSmartFilter(t, custom.filter);
    return null;
  });

  const filteredActive = $derived(
    taskStore.activeTasks
      .filter(t =>
        projectFilter === "all" ? true :
        projectFilter === "none" ? !t.project_id :
        t.project_id === projectFilter
      )
      .filter(t => activeSmartListTest ? activeSmartListTest(t) : true)
  );

  // The board uses the same project and smart-list filters as the list, but over
  // taskStore.tasks rather than activeTasks: completed tasks (hidden=true, the same
  // flag that moves them into History in list mode) must stay visible in their own
  // column rather than vanishing from the whole board.
  const boardTasks = $derived(
    taskStore.tasks
      .filter(t => t.status !== "Archived")
      .filter(t =>
        projectFilter === "all" ? true :
        projectFilter === "none" ? !t.project_id :
        t.project_id === projectFilter
      )
      .filter(t => activeSmartListTest ? activeSmartListTest(t) : true)
  );

  // The multi-selection does not survive a change of the visible list (filter,
  // search, switching smart lists), otherwise a bulk action could quietly affect
  // rows that are no longer on screen.
  $effect(() => {
    const visible = new Set(filteredActive.map(t => t.id));
    if ([...selectedIds].some(id => !visible.has(id))) {
      selectedIds = new Set([...selectedIds].filter(id => visible.has(id)));
    }
  });

  // Grouping by "all projects": one section per project (in the projects' own
  // order) plus "No project".
  const grouped = $derived.by(() => {
    if (projectFilter !== "all" || projectStore.projects.length === 0) return null;
    const groups: { id: string; name: string; done: number; total: number; tasks: Task[]; project: Project | null }[] = [];
    for (const p of projectStore.projects) {
      const tasks = filteredActive.filter(t => t.project_id === p.id);
      if (tasks.length > 0) {
        groups.push({ id: p.id, name: p.name, done: p.task_done, total: p.task_total, tasks, project: p });
      }
    }
    const orphan = filteredActive.filter(t => !t.project_id || !projectStore.projects.some(p => p.id === t.project_id));
    if (orphan.length > 0 && groups.length > 0) {
      groups.push({ id: "", name: t("Без проекта"), done: 0, total: 0, tasks: orphan, project: null });
    }
    return groups.length > 0 ? groups : null;
  });

  // A project's goal: the progress text "done/target tasks · done/target min" and its status
  function goalText(p: Project): string | null {
    if (p.goal_tasks == null && p.goal_mins == null) return null;
    const parts: string[] = [];
    if (p.goal_tasks != null) parts.push(t("{done}/{total} задач", { done: p.goal_done_tasks, total: p.goal_tasks }));
    if (p.goal_mins != null) parts.push(t("{done}/{total} мин", { done: p.goal_done_mins, total: p.goal_mins }));
    return parts.join(" · ");
  }

  function goalMet(p: Project): boolean {
    return (p.goal_tasks == null || p.goal_done_tasks >= p.goal_tasks)
        && (p.goal_mins == null || p.goal_done_mins >= p.goal_mins);
  }

  async function toggleGoalHistory(projectId: string) {
    if (showGoalHistory[projectId]) {
      const next = { ...showGoalHistory };
      delete next[projectId];
      showGoalHistory = next;
      return;
    }
    goalHistoryLoading = { ...goalHistoryLoading, [projectId]: true };
    try {
      const snapshots = await api.getGoalHistory(projectId);
      showGoalHistory = { ...showGoalHistory, [projectId]: snapshots };
    } finally {
      goalHistoryLoading = { ...goalHistoryLoading, [projectId]: false };
    }
  }

  async function addProject() {
    const name = newProjectName.trim();
    if (!name) return;
    await projectStore.create(name);
    newProjectName = "";
  }

  // The day's schedule: today's time blocks (assigned in Calendar -> Week)
  const todayBlocks = $derived.by(() => {
    const today = new Date().toDateString();
    return taskStore.activeTasks
      .filter(t => t.scheduled_at && new Date(t.scheduled_at).toDateString() === today)
      .sort((a, b) => a.scheduled_at!.localeCompare(b.scheduled_at!));
  });

  function blockTime(t: Task): string {
    const start = new Date(t.scheduled_at!);
    const end = new Date(start.getTime() + (t.scheduled_mins ?? 60) * 60_000);
    const fmt = (d: Date) => `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    return `${fmt(start)}–${fmt(end)}`;
  }

  let searchQuery = $state("");
  let searchResults = $state<Task[]>([]);
  let isSearching = $state(false);

  let aiLoadingId: string | null = $state(null);
  let aiError: string | null = $state(null);
  let subtasksPreview: { taskId: string; items: string[] } | null = $state(null);

  let trackingId: string | null = $state(null);

  onMount(() => {
    api.getActiveSession().then(s => { trackingId = s?.task_id ?? null; }).catch(() => {});
  });

  // Completing via the row's ✓. Tracking must be stopped explicitly here: this path
  // bypasses moveToStatus (which stops it the same way when leaving InProgress),
  // and without that the timer kept ticking on an already-completed task.
  async function completeRow(task: Task) {
    if (trackingId === task.id) {
      await api.stopTaskTracking();
      trackingId = null;
    }
    // An unsaved checklist edit is flushed BEFORE completing: the line below drops
    // the panel's cache, and without the flush the edit would go with it. Verified:
    // renaming while completing "immediately" lost not the text but the subtask
    // itself — an empty list was left in the DB.
    await flushSubs(task);
    await taskStore.complete(task.id);
    // The panel's cache must go: it lives separately from the store, so after the
    // checklist is reset (a recurring task moving to its next run) the screen would
    // keep ticks that no longer exist in the DB. It also fixes a race — a deferred
    // write arriving after the reset finds no text and does not restore the ticks.
    delete subsText[task.id];
    projectStore.load();
  }

  async function toggleTracking(taskId: string) {
    if (trackingId === taskId) {
      await api.stopTaskTracking();
      trackingId = null;
    } else {
      await api.startTaskTracking(taskId);
      trackingId = taskId;
    }
    taskStore.load();
  }

  // --- The board: one column per status from statusStore rather than a hardcoded
  // Todo/InProgress/Done, since the user can add their own. ---
  function boardTasksFor(statusId: string): Task[] {
    return boardTasks
      .filter(t => t.status === statusId)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  }

  // Drag and drop: card to column (not card to card, as in the manual list sorting
  // above) — one dropzone per column, with no manual ordering inside it (sorted by
  // updated_at).
  let boardDragTaskId: string | null = $state(null);
  let boardDropTargetStatus: string | null = $state(null);

  function cardDragStart(e: DragEvent, task: Task) {
    boardDragTaskId = task.id;
    e.dataTransfer?.setData("text/plain", task.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function columnDragOver(e: DragEvent, statusId: string) {
    if (!boardDragTaskId) return;
    e.preventDefault();
    boardDropTargetStatus = statusId;
  }

  async function columnDrop(e: DragEvent, statusId: string) {
    e.preventDefault();
    const taskId = boardDragTaskId ?? e.dataTransfer?.getData("text/plain");
    boardDragTaskId = null;
    boardDropTargetStatus = null;
    if (!taskId) return;
    const task = taskStore.tasks.find(t => t.id === taskId);
    if (!task || task.status === statusId) return;
    await moveToStatus(task, statusId);
  }

  // InProgress and Done are special cases with side effects (time tracking,
  // completion) — see api.completeTask/startTaskTracking; every other status,
  // including user-defined ones, is a plain update_task.
  async function moveToStatus(task: Task, statusId: string) {
    if (task.status === "InProgress" && statusId !== "InProgress" && trackingId === task.id) {
      await api.stopTaskTracking();
      trackingId = null;
    }
    if (statusId === "Done") {
      await api.completeTask(task.id);
    } else if (statusId === "InProgress") {
      await api.startTaskTracking(task.id);
      trackingId = task.id;
    } else {
      await api.updateTask(task.id, { status: statusId });
    }
    await taskStore.load();
  }

  let boardCreateStatus = $state("Todo");

  function openBoardCreate(statusId: string) {
    boardCreateStatus = statusId;
    showCreateModal = true;
  }

  // "+ Column" right on the board: a quick way to add a status without going to
  // Settings (renaming and deletion stay there only, see "Task statuses" in
  // Settings.svelte).
  let showStatusQuickAdd = $state(false);
  let newBoardStatusName = $state("");

  async function addBoardStatus() {
    const name = newBoardStatusName.trim();
    if (!name) return;
    await statusStore.create(name, "#888888");
    newBoardStatusName = "";
    showStatusQuickAdd = false;
  }

  // Opening a task on an external signal (global search via Ctrl+K, the day popup
  // in the Dashboard or Calendar). A completed task (hidden) is history, so we open
  // the read-only TaskHistoryDetail rather than the editable TaskModal: otherwise
  // clicking a completed task in the day popup would open it as active for editing,
  // and a deadline or recurrence no longer means anything for something long done.
  $effect(() => {
    const id = taskStore.focusTaskId;
    if (!id) return;
    const task = taskStore.tasks.find(t => t.id === id);
    if (task) {
      if (task.hidden) historyDetailTask = task;
      else editingTask = task;
    }
    taskStore.clearFocus();
  });

  async function handleCreate(data: CreateTaskPayload | UpdateTaskPayload) {
    const payload = data as CreateTaskPayload;
    const created = await taskStore.create(payload);
    // Creating straight into InProgress (via "+ column" on the board, for one): the
    // status is already set by the modal (initialStatus), but the actual tracking
    // timer is started by a separate call, as everywhere else in the app.
    if (created && payload.status === "InProgress") {
      await api.startTaskTracking(created.id);
      trackingId = created.id;
      await taskStore.load();
    }
    return created;
  }

  // --- The inline composer: the first line is the title, Enter inserts a line
  // break, Shift+Enter adds a subtask line (☐), Ctrl+Enter creates the task. ---
  let composerText = $state("");
  let composerEl: HTMLTextAreaElement | undefined = $state();
  let composerBusy = $state(false);
  const composerRows = $derived(Math.min(6, composerText.split("\n").length));

  // Natural language in the title: !priority / @category / #tag and relative
  // dates and times are parsed live from the first line as it is typed.
  const composerDraft = $derived(parseComposer(composerText));
  const composerMeta = $derived(parseTaskText(composerDraft.title));
  const composerCategoryId = $derived(
    composerMeta.categoryQuery ? matchCategoryQuery(categoryStore.categories, composerMeta.categoryQuery) : null
  );

  function composerInsertSubtaskLine() {
    const el = composerEl;
    if (!el) return;
    const start = el.selectionStart;
    const insert = "\n" + SUBTASK_PREFIX;
    composerText = composerText.slice(0, start) + insert + composerText.slice(el.selectionEnd);
    tick().then(() => {
      el.setSelectionRange(start + insert.length, start + insert.length);
    });
  }

  function composerKeydown(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    if (e.shiftKey) {
      e.preventDefault();
      composerInsertSubtaskLine();
    } else if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      submitComposer();
    }
    // a plain Enter is the default line break
  }

  async function submitComposer() {
    const draft = parseComposer(composerText);
    if (!draft.title || composerBusy) return;
    const meta = parseTaskText(draft.title);
    composerBusy = true;
    try {
      // The active project filter is a sensible default for a new task
      const projectId = projectFilter !== "all" && projectFilter !== "none" ? projectFilter : null;
      const categoryId = meta.categoryQuery ? matchCategoryQuery(categoryStore.categories, meta.categoryQuery) : null;
      const task = await api.createTask({
        title: meta.title || draft.title,
        description: draft.description || null,
        status: "Todo",
        priority: meta.priority ?? "Medium",
        category: categoryId ?? "Other", // the fallback category always exists (Work can be deleted)
        deadline: meta.deadline ? meta.deadline.toISOString() : null,
        tags: meta.tags,
        recurrence: "None",
        project_id: projectId,
      });
      for (const sub of draft.subtasks) {
        await api.addSubtask(task.id, sub);
      }
      composerText = "";
      await taskStore.load();
    } catch (e) {
      aiError = typeof e === "string" ? e : t("Не удалось создать задачу");
    }
    composerBusy = false;
    composerEl?.focus();
  }

  async function handleEdit(data: CreateTaskPayload | UpdateTaskPayload) {
    if (!editingTask) return;
    await taskStore.update(editingTask.id, data as UpdateTaskPayload);
  }

  async function handleSearch() {
    if (!searchQuery.trim()) { searchResults = []; return; }
    isSearching = true;
    searchResults = await taskStore.search(searchQuery);
    isSearching = false;
  }

  async function rewriteTask(id: string, title: string) {
    aiLoadingId = id;
    aiError = null;
    await api.aiRewrite(id, title);
  }

  async function generateSubtasks(id: string, title: string) {
    aiLoadingId = id;
    aiError = null;
    subtasksPreview = null;
    await api.aiSubtasks(id, title);
  }

  // Add a single AI-suggested subtask as a checklist item under its parent task
  async function acceptSubtask(parentId: string, title: string) {
    await api.addSubtask(parentId, title);
    await taskStore.load();
  }

  // Accept every suggested subtask at once
  async function acceptAllSubtasks(parentId: string, items: string[]) {
    for (const title of items) {
      await api.addSubtask(parentId, title);
    }
    subtasksPreview = null;
    await taskStore.load();
  }

  async function toggleSubtask(id: string) {
    await api.toggleSubtask(id);
    await taskStore.load();
  }

  // --- The checklist in a row's panel. The `[x] ` markup is hidden behind a
  // checkbox inside the line, as in the modal and the quick slot — the requirement
  // was to change it the same way everywhere.
  //
  // Writing here is immediate (the panel is opened to tick something and close),
  // but writing to the DB on every keystroke is not an option, hence a typing pause
  // as in the slot. Each task gets its own pause: several rows can be expanded at once.
  const SUBS_DEBOUNCE_MS = 600;
  let subsText = $state<Record<string, string>>({});
  let subsTimers: Record<string, ReturnType<typeof setTimeout>> = {};
  let subsBusy = $state<Record<string, boolean>>({});

  // The panel's text is kept separately from the store: while the user types the
  // store is re-read (by a neighbouring task, for instance) and would clobber the
  // edit. It is initialized on expansion rather than in a $derived.
  function subsTextFor(task: Task): string {
    return subsText[task.id]
      ?? formatChecklist(task.subtasks.map(s => ({ title: s.title, done: s.done })));
  }

  function scheduleSubsFlush(task: Task) {
    clearTimeout(subsTimers[task.id]);
    subsTimers[task.id] = setTimeout(() => flushSubs(task), SUBS_DEBOUNCE_MS);
  }

  // The same positional diff as in the modal and the slot: line i edits subtask i.
  // An error is shown as a banner and the text is left as is, so the edit does not
  // disappear silently.
  async function flushSubs(task: Task) {
    clearTimeout(subsTimers[task.id]);
    if (subsBusy[task.id]) return;
    const current = parseChecklist(subsText[task.id] ?? "");
    const orig = task.subtasks;
    const same =
      current.length === orig.length &&
      current.every((c, i) => c.title === orig[i].title && c.done === orig[i].done);
    if (same) return;
    subsBusy[task.id] = true;
    try {
      await taskStore.guarded(async () => {
        for (let i = current.length; i < orig.length; i++) {
          await api.deleteSubtask(orig[i].id);
        }
        for (let i = 0; i < current.length; i++) {
          const c = current[i];
          const o = orig[i];
          if (!o) {
            const added = await api.addSubtask(task.id, c.title);
            if (c.done) await api.toggleSubtask(added.id);
          } else {
            if (o.title !== c.title) await api.renameSubtask(o.id, c.title);
            if (o.done !== c.done) await api.toggleSubtask(o.id);
          }
        }
      });
      await taskStore.load();
    } finally {
      subsBusy[task.id] = false;
    }
  }

  let expanded = $state<Record<string, boolean>>({});

  // An explicit click overrides auto-expansion; without one, tasks with subtasks
  // are open when the show_subtasks_expanded setting is on.
  function isExpanded(task: Task): boolean {
    return expanded[task.id] ?? (autoExpandSubs && task.subtasks.length > 0);
  }

  // --- Manual sorting: dragging a row within its own list (group) ---
  let dragTaskId: string | null = $state(null);
  let dropTargetId: string | null = $state(null);

  // --- Multi-select: Ctrl/Shift+click on a row instead of opening the card. Ctrl
  // toggles one row, Shift selects a range from the last selected row within the
  // currently visible list (ignoring grouping — a flat order).
  let selectedIds = $state<Set<string>>(new Set());
  let lastSelectedId: string | null = $state(null);
  let bulkBusy = $state(false);
  let bulkProjectId = $state("");
  let bulkCategory = $state("");

  function visibleTaskIds(): string[] {
    if (grouped) return grouped.flatMap(g => g.tasks.map(t => t.id));
    return filteredActive.map(t => t.id);
  }

  function toggleSelect(task: Task, e: MouseEvent) {
    const ids = visibleTaskIds();
    if (e.shiftKey && lastSelectedId) {
      const from = ids.indexOf(lastSelectedId);
      const to = ids.indexOf(task.id);
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const next = new Set(selectedIds);
        for (let i = lo; i <= hi; i++) next.add(ids[i]);
        selectedIds = next;
        return;
      }
    }
    const next = new Set(selectedIds);
    if (next.has(task.id)) next.delete(task.id); else next.add(task.id);
    selectedIds = next;
    lastSelectedId = task.id;
  }

  function onRowClick(e: MouseEvent, task: Task) {
    if (e.ctrlKey || e.metaKey || e.shiftKey) {
      e.preventDefault();
      toggleSelect(task, e);
      return;
    }
    editingTask = task;
  }

  function clearSelection() {
    selectedIds = new Set();
    lastSelectedId = null;
  }

  async function bulkComplete() {
    bulkBusy = true;
    try {
      await Promise.all([...selectedIds].map(id => api.completeTask(id)));
      await taskStore.load();
      clearSelection();
    } finally {
      bulkBusy = false;
    }
  }

  async function bulkDelete() {
    bulkBusy = true;
    try {
      await Promise.all([...selectedIds].map(id => api.deleteTask(id)));
      await taskStore.load();
      clearSelection();
    } finally {
      bulkBusy = false;
    }
  }

  async function bulkMoveToProject() {
    if (!bulkProjectId) return;
    bulkBusy = true;
    try {
      const project_id = bulkProjectId === "none" ? "" : bulkProjectId;
      await Promise.all([...selectedIds].map(id => api.updateTask(id, { project_id })));
      await taskStore.load();
      clearSelection();
      bulkProjectId = "";
    } finally {
      bulkBusy = false;
    }
  }

  async function bulkSetCategory() {
    if (!bulkCategory) return;
    bulkBusy = true;
    try {
      await Promise.all([...selectedIds].map(id => api.updateTask(id, { category: bulkCategory as Category })));
      await taskStore.load();
      clearSelection();
      bulkCategory = "";
    } finally {
      bulkBusy = false;
    }
  }

  function listForTask(task: Task): Task[] {
    if (grouped) {
      const g = grouped.find(g => g.tasks.some(t => t.id === task.id));
      return g ? g.tasks : [];
    }
    return filteredActive;
  }

  function rowDragStart(e: DragEvent, task: Task) {
    dragTaskId = task.id;
    e.dataTransfer?.setData("text/plain", task.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function rowDragOver(e: DragEvent, task: Task) {
    if (!dragTaskId || dragTaskId === task.id) return;
    e.preventDefault();
    dropTargetId = task.id;
  }

  async function rowDrop(e: DragEvent, target: Task) {
    e.preventDefault();
    const sourceId = dragTaskId ?? e.dataTransfer?.getData("text/plain");
    dragTaskId = null;
    dropTargetId = null;
    if (!sourceId || sourceId === target.id) return;
    const ids = listForTask(target).map(t => t.id);
    const from = ids.indexOf(sourceId);
    const to = ids.indexOf(target.id);
    if (from < 0 || to < 0) return; // dragging between groups is not sorting
    ids.splice(from, 1);
    ids.splice(to, 0, sourceId);
    await taskStore.reorder(ids);
  }
  const doneCount = (t: Task) => t.subtasks.filter((s) => s.done).length;

  async function classifyTask(id: string, title: string) {
    aiLoadingId = id;
    aiError = null;
    await api.aiClassify(id, title);
  }

  const PRIORITY_LABELS: Record<string, string> = $derived({
    Low: t("Низкий"), Medium: t("Средний"), High: t("Высокий"), Critical: t("Критический"),
  });

  function recurrenceLabel(r: unknown): string | null {
    if (!r || r === "None") return null;
    if (r === "Hourly") return t("Каждый час");
    if (r === "Daily")  return t("Каждый день");
    if (r === "Weekly") return t("Каждую неделю");
    if (typeof r === "object" && r !== null && "Custom" in r) {
      const [n, unit] = (r as any).Custom;
      const unitLabel =
        unit === "Minutes" ? t("мин.") :
        unit === "Hours"   ? t("ч.") :
        unit === "Days"    ? t("дн.") : t("нед.");
      return t("раз в {n} {unit}", { n, unit: unitLabel });
    }
    if (typeof r === "object" && r !== null && "Weekdays" in r) {
      const labels = [t("Пн"), t("Вт"), t("Ср"), t("Чт"), t("Пт"), t("Сб"), t("Вс")];
      const mask = (r as any).Weekdays as number;
      const days = labels.filter((_, i) => mask & (1 << i));
      return t("по {days}", { days: days.join(", ") });
    }
    return null;
  }

  // A compact deadline: "today 18:00", "tomorrow", "3 d", "2 d overdue"
  function deadlineInfo(iso: string): { label: string; overdue: boolean } {
    const d = new Date(iso);
    const now = new Date();
    const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
    const dayDiff = Math.round((startOfDay(d) - startOfDay(now)) / 864e5);

    if (d.getTime() < now.getTime()) {
      return { label: dayDiff === 0 ? t("просрочено") : t("просрочено {n} дн", { n: -dayDiff }), overdue: true };
    }
    if (dayDiff === 0) {
      return { label: t("сегодня {time}", { time: d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) }), overdue: false };
    }
    if (dayDiff === 1) return { label: t("завтра"), overdue: false };
    if (dayDiff < 7) return { label: t("{n} дн", { n: dayDiff }), overdue: false };
    return { label: d.toLocaleDateString([], { day: "numeric", month: "short" }), overdue: false };
  }

  taskStore.load();

  onMount(() => {
    const unlistenAi = listen<AiResult>("ai-result", async ({ payload }) => {
      if (payload.error) {
        aiLoadingId = null;
        aiError = payload.error;
        return;
      }
      if (!payload.result) { aiLoadingId = null; return; }

      if (payload.type === "rewrite") {
        await taskStore.update(payload.task_id, { title: payload.result });
        aiLoadingId = null;
      } else if (payload.type === "subtasks") {
        const items = payload.result.split("|||").filter(Boolean);
        subtasksPreview = { taskId: payload.task_id, items };
        aiLoadingId = null;
      } else if (payload.type === "classify") {
        const valid = ["Work","Study","Home","Health","Other"];
        if (valid.includes(payload.result)) {
          await taskStore.update(payload.task_id, { category: payload.result as Category });
        }
        aiLoadingId = null;
      }
    });

    const unlistenWhatNow = listen<{ result: string | null; error: string | null }>("ai-what-now", ({ payload }) => {
      whatNowPending = false;
      whatNow = payload.result;
      if (payload.error) aiError = payload.error;
    });

    return () => {
      unlistenAi.then(fn => fn());
      unlistenWhatNow.then(fn => fn());
    };
  });

  // "What should I do now": AI advice from the current context (blocks, deadlines, priorities)
  let whatNow: string | null = $state(null);
  let whatNowPending = $state(false);

  async function askWhatNow() {
    whatNowPending = true;
    whatNow = null;
    aiError = null;
    try {
      await api.aiWhatNow();
    } catch (e) {
      whatNowPending = false;
      aiError = String(e);
    }
  }
</script>

{#snippet taskRow(task: Task)}
  {@const busy = aiLoadingId === task.id}
  {@const blocked = task.blocked_by.length > 0}
  {@const blockerNames = task.blocked_by.map(b => b.title).join(", ")}
  <li
    class="task-row"
    style="--prio: var(--prio-{task.priority.toLowerCase()});"
    class:dragging={dragTaskId === task.id}
    class:drop-target={dropTargetId === task.id}
    class:selected={selectedIds.has(task.id)}
    class:blocked
    draggable={!searchQuery.trim() && !task.hidden}
    ondragstart={(e) => rowDragStart(e, task)}
    ondragover={(e) => rowDragOver(e, task)}
    ondrop={(e) => rowDrop(e, task)}
    ondragend={() => { dragTaskId = null; dropTargetId = null; }}
  >
    <!-- A blocked task cannot be completed. The backend forbids it too, but
         disabled here keeps a click from producing an error. -->
    <button
      class="task-check"
      onclick={() => completeRow(task)}
      disabled={blocked}
      title={blocked ? t("Заблокирована: {tasks}", { tasks: blockerNames }) : t("Выполнить")}
      aria-label={t("Выполнить задачу")}
    ></button>

    <div
      class="task-main"
      onclick={(e) => onRowClick(e, task)}
      onkeydown={(e) => { if (e.key === "Enter") editingTask = task; }}
      role="button"
      tabindex="0"
    >
      <div class="task-title">
        <span class="prio-dot" title="{t('Приоритет')}: {PRIORITY_LABELS[task.priority]}"></span>
        {task.title}
        {#if recurrenceLabel(task.recurrence)}
          <span class="muted" title={recurrenceLabel(task.recurrence)}>↻</span>
        {/if}
      </div>
      {#if task.description}
        <div class="task-desc">{task.description}</div>
      {/if}
      <!-- The reason is spelled out rather than only dimmed: otherwise it is
           unclear why the task's checkmark will not click. -->
      {#if blocked}
        <div class="task-blocked-by">{t("Заблокирована: {tasks}", { tasks: blockerNames })}</div>
      {/if}
    </div>

    <div class="task-meta">
      <button
        class="chip chip-sub"
        class:has-subs={task.subtasks.length > 0}
        class:subs-done={task.subtasks.length > 0 && doneCount(task) === task.subtasks.length}
        onclick={() => expanded[task.id] = !isExpanded(task)}
        title={task.subtasks.length > 0 ? t("Подзадачи") : t("Добавить подзадачу")}
      >{isExpanded(task) ? "▾" : "▸"}
        {#if task.subtasks.length > 0}
          <span class="sub-track"><span class="sub-fill" style="width:{Math.round(doneCount(task) / task.subtasks.length * 100)}%"></span></span>
          {doneCount(task)}/{task.subtasks.length}
        {:else}+{/if}</button>
      {#each task.tags as tag}
        <span class="chip chip-tag">#{tag}</span>
      {/each}
      <span class="chip chip-cat" style="--cat: {categoryStore.color(task.category)}">{categoryStore.name(task.category)}</span>
      {#if task.deadline}
        {@const dl = deadlineInfo(task.deadline)}
        <span class="chip" class:chip-danger={dl.overdue}><Icon name="flag" size={11} /> {dl.label}</span>
      {/if}
    </div>

    <div class="task-actions">
      <button class="btn-icon" disabled={busy} title={t("Переформулировать в SMART")}
        onclick={() => rewriteTask(task.id, task.title)}>{#if busy}…{:else}<Icon name="sparkles" />{/if}</button>
      <button class="btn-icon" disabled={busy} title={t("Разбить на подзадачи")}
        onclick={() => generateSubtasks(task.id, task.title)}>{#if busy}…{:else}<Icon name="shuffle" />{/if}</button>
      <button class="btn-icon" disabled={busy} title={t("Авто-категория")}
        onclick={() => classifyTask(task.id, task.title)}>{#if busy}…{:else}<Icon name="tag" />{/if}</button>
      <button class="btn-icon" title={trackingId === task.id ? t("Остановить трекинг") : t("Начать трекинг")}
        onclick={() => toggleTracking(task.id)} class:active={trackingId === task.id}>
        {#if trackingId === task.id}<Icon name="stop" />{:else}<Icon name="play" />{/if}</button>
      <button class="btn-icon" class:active={pinnedStore.is("task", task.id)}
        title={pinnedStore.is("task", task.id) ? t("Убрать из быстрого слота") : t("В быстрый слот (Ctrl+Shift+J)")}
        onclick={() => pinnedStore.toggle("task", task.id)}><Icon name="zap" /></button>
      <button class="btn-icon btn-danger" title={t("Удалить")}
        onclick={() => taskStore.remove(task.id)}>✕</button>
    </div>
  </li>

  {#if subtasksPreview && subtasksPreview.taskId === task.id}
    <li class="task-sub-panel">
      <div class="sub-preview-head">
        <span class="section-title" style="margin:0;">{t("ИИ предлагает подзадачи")}</span>
        <div style="display:flex;gap:6px;">
          <button class="btn-sm btn-primary" onclick={() => acceptAllSubtasks(task.id, subtasksPreview!.items)}>{t("Принять все")}</button>
          <button class="btn-sm" onclick={() => subtasksPreview = null}>{t("Закрыть")}</button>
        </div>
      </div>
      {#each subtasksPreview.items as subtask}
        <div class="sub-line">
          <span style="flex:1;">{subtask}</span>
          <button class="btn-sm" onclick={() => acceptSubtask(task.id, subtask)}>{t("+ Добавить")}</button>
        </div>
      {/each}
    </li>
  {/if}

  {#if isExpanded(task)}
    <li class="task-sub-panel">
      <ChecklistEditor
        value={subsTextFor(task)}
        placeholder={t("Подзадача на строку (Enter — ещё строка)")}
        onchange={(text) => { subsText[task.id] = text; scheduleSubsFlush(task); }}
      />
    </li>
  {/if}
{/snippet}

<!-- Modals -->
{#if showCreateModal}
  <TaskModal
    initialStatus={boardCreateStatus}
    onSave={handleCreate}
    onClose={() => showCreateModal = false}
  />
{/if}

{#if editingTask}
  <TaskModal
    task={editingTask}
    onSave={handleEdit}
    onClose={() => editingTask = null}
  />
{/if}

{#if historyDetailTask}
  <TaskHistoryDetail
    task={historyDetailTask}
    onClose={() => historyDetailTask = null}
  />
{/if}

{#if showProjects}
  <div role="dialog" aria-modal="true" class="overlay backdrop"
    onclick={(e) => { if (e.target === e.currentTarget) showProjects = false; }}>
    <div class="modal dialog">
      <h2 class="dialog-title">{t("Проекты")}</h2>

      {#if projectStore.error}
        <div class="alert" style="margin:0;">{projectStore.error}</div>
      {/if}

      {#each projectStore.projects as p (p.id)}
        <div class="proj-row" class:archived={p.archived}>
          <input
            value={p.name}
            onchange={(e) => projectStore.update(p.id, { name: e.currentTarget.value })}
          />
          <span class="muted proj-progress">{p.task_done}/{p.task_total}</span>
          <button class="btn-sm" title={p.archived ? t("Разархивировать") : t("В архив")}
            onclick={() => projectStore.update(p.id, { archived: !p.archived })}>
            {p.archived ? t("Вернуть") : t("Архив")}
          </button>
          <button class="btn-icon btn-danger" title={t("Удалить проект (задачи останутся без проекта)")}
            onclick={() => projectStore.remove(p.id)}>✕</button>
        </div>
        {#if !p.archived}
          <div class="proj-goal">
            <span class="muted">{t("Цель:")}</span>
            <input class="goal-num" type="number" min="0" placeholder="—"
              value={p.goal_tasks ?? ""}
              onchange={(e) => projectStore.update(p.id, { goal_tasks: Number(e.currentTarget.value) || 0 })}
            />
            <span class="muted">{t("задач ·")}</span>
            <input class="goal-num" type="number" min="0" step="15" placeholder="—"
              value={p.goal_mins ?? ""}
              onchange={(e) => projectStore.update(p.id, { goal_mins: Number(e.currentTarget.value) || 0 })}
            />
            <span class="muted">{t("мин в")}</span>
            <select
              value={p.goal_period}
              onchange={(e) => projectStore.update(p.id, { goal_period: e.currentTarget.value as "week" | "month" })}
            >
              <option value="week">{t("неделю")}</option>
              <option value="month">{t("месяц")}</option>
            </select>
            {#if goalText(p)}
              <span class="goal-chip" class:met={goalMet(p)}>{goalText(p)}</span>
              <button class="btn-sm" onclick={() => toggleGoalHistory(p.id)}>
                {showGoalHistory[p.id] ? t("Скрыть") : t("История")}
              </button>
            {/if}
            {#if showGoalHistory[p.id]}
              <div class="goal-history">
                {#if goalHistoryLoading[p.id]}
                  <span class="muted">{t("Загрузка…")}</span>
                {:else if showGoalHistory[p.id].length === 0}
                  <span class="muted">{t("Нет записей")}</span>
                {:else}
                  {#each showGoalHistory[p.id] as snap (snap.id)}
                    <div class="goal-history-row">
                      <span class="muted">{snap.recorded_at.slice(0, 16)}</span>
                      <span>{snap.done_tasks}{snap.goal_tasks != null ? `/${snap.goal_tasks}` : ''} {t("задач")}</span>
                      <span>·</span>
                      <span>{snap.done_mins}{snap.goal_mins != null ? `/${snap.goal_mins}` : ''} {t("мин")}</span>
                    </div>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      {:else}
        <p class="muted" style="margin:0;font-size:13px;">{t("Проектов пока нет — создайте первый.")}</p>
      {/each}

      <div class="proj-row">
        <input
          bind:value={newProjectName}
          placeholder={t("Название нового проекта")}
          onkeydown={(e) => { if (e.key === "Enter") addProject(); }}
        />
        <button class="btn-primary" onclick={addProject} disabled={!newProjectName.trim()}>{t("Создать")}</button>
      </div>

      <div class="actions">
        <button class="btn-ghost" onclick={() => showProjects = false}>{t("Закрыть")}</button>
      </div>
    </div>
  </div>
{/if}

{#if showSmartListModal}
  <div role="dialog" aria-modal="true" class="overlay backdrop"
    onclick={(e) => { if (e.target === e.currentTarget) { showSmartListModal = false; resetSmartListForm(); } }}>
    <div class="modal dialog">
      <h2 class="dialog-title">{t("Новый умный список")}</h2>

      {#if smartListStore.error}
        <div class="alert" style="margin:0;">{smartListStore.error}</div>
      {/if}

      <label class="field">
        <span class="label">{t("Название")}</span>
        <input bind:value={newSmartListName} placeholder={t("Например: Важное")} />
      </label>

      <div class="pair" style="margin-top:8px;">
        <label class="field">
          <span class="label">{t("Категория")}</span>
          <select bind:value={newSmartListCategory}>
            <option value="">{t("Любая")}</option>
            {#each categoryStore.categories as c (c.id)}
              <option value={c.id}>{categoryStore.name(c.id)}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span class="label">{t("Приоритет")}</span>
          <select bind:value={newSmartListPriority}>
            <option value="">{t("Любой")}</option>
            {#each Object.entries(PRIORITY_LABELS) as [value, label] (value)}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>
      </div>

      <div class="pair" style="margin-top:8px;">
        <label class="field">
          <span class="label">{t("Тег")}</span>
          <input bind:value={newSmartListTag} placeholder={t("без #")} />
        </label>
        <label class="field">
          <span class="label">{t("Дедлайн")}</span>
          <select bind:value={newSmartListHasDeadline}>
            <option value="">{t("Не важно")}</option>
            <option value="yes">{t("Есть дедлайн")}</option>
            <option value="no">{t("Без дедлайна")}</option>
          </select>
        </label>
      </div>

      <p class="hint">{t("Условия комбинируются через «И» — задача должна подойти под все заданные.")}</p>

      <div class="actions">
        <button class="btn-ghost" onclick={() => { showSmartListModal = false; resetSmartListForm(); }}>{t("Отмена")}</button>
        <button class="btn-primary" onclick={createSmartList} disabled={!newSmartListName.trim()}>{t("Создать")}</button>
      </div>
    </div>
  </div>
{/if}

<div class="page" class:board-mode={viewMode === "board"}>
  <div class="page-head">
    <h1 class="page-title">{t("Задачи")}</h1>
    <span class="muted count">
      {t("{active} актив. · {history} в истории", {
        active: taskStore.activeTasks.length,
        history: taskStore.historyTasks.length,
      })}
    </span>
    <div class="seg">
      <button class:active={viewMode === "list"} onclick={() => viewMode = "list"}>{t("Список")}</button>
      <button class:active={viewMode === "board"} onclick={() => viewMode = "board"}>{t("Доска")}</button>
    </div>
    <span style="flex:1;"></span>
    <input
      bind:value={searchQuery}
      oninput={handleSearch}
      placeholder={t("Поиск задач…")}
      class="head-search"
    />
    {#if projectStore.projects.length > 0}
      <select bind:value={projectFilter} class="project-filter" title={t("Фильтр по проекту")}>
        <option value="all">{t("Все проекты")}</option>
        <option value="none">{t("Без проекта")}</option>
        {#each projectStore.active as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
    {/if}
    {#if aiEnabled}
      <button onclick={askWhatNow} disabled={whatNowPending}
        title={t("ИИ посоветует, чем заняться сейчас — по блокам, дедлайнам и приоритетам")}>
        {#if whatNowPending}{t("Думаю…")}{:else}<Icon name="target" size={12} /> {t("Что сейчас?")}{/if}
      </button>
    {/if}
    <button onclick={() => { showProjects = true; projectStore.load(); }}>{t("Проекты")}</button>
    <div class="seg">
      <button class:active={listSubView === "active"} onclick={() => listSubView = "active"}>{t("Активные")}</button>
      <button class:active={listSubView === "history"} onclick={() => listSubView = "history"}>{t("История")}</button>
      <button class:active={listSubView === "trash"} onclick={() => { listSubView = "trash"; taskStore.loadDeleted(); }}>{t("Корзина")}</button>
    </div>
    <button class="btn-primary" onclick={() => { boardCreateStatus = "Todo"; showCreateModal = true; }}>{t("+ Новая")}</button>
  </div>

  <!-- Store errors are finally visible. taskStore.error used to be set but never
       rendered anywhere, so a failed operation looked like "the button does not
       work", with no sign that anything had gone wrong (that is exactly how the
       recurrence bug presented). This is the same inline .alert already used in
       Notes and Settings. -->
  {#if taskStore.error}
    <div class="alert task-error" role="alert">
      <span>{taskStore.error}</span>
      <button class="btn-sm" onclick={() => taskStore.clearError()} title={t("Скрыть")}>✕</button>
    </div>
  {/if}

  {#if selectedIds.size > 0}
    <div class="bulk-bar card">
      <span class="bulk-count">{t("{n} выбрано", { n: selectedIds.size })}</span>
      <select bind:value={bulkProjectId} disabled={bulkBusy} title={t("Перенести в проект")}>
        <option value="" disabled selected>{t("В проект…")}</option>
        <option value="none">{t("Без проекта")}</option>
        {#each projectStore.active as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
      {#if bulkProjectId}
        <button class="btn-sm" disabled={bulkBusy} onclick={bulkMoveToProject}>{t("Перенести")}</button>
      {/if}
      <select bind:value={bulkCategory} disabled={bulkBusy} title={t("Сменить категорию")}>
        <option value="" disabled selected>{t("Категория…")}</option>
        {#each categoryStore.categories as c (c.id)}
          <option value={c.id}>{categoryStore.name(c.id)}</option>
        {/each}
      </select>
      {#if bulkCategory}
        <button class="btn-sm" disabled={bulkBusy} onclick={bulkSetCategory}>{t("Применить")}</button>
      {/if}
      <button class="btn-sm" disabled={bulkBusy} onclick={bulkComplete}>{t("Выполнить")}</button>
      <button class="btn-sm btn-danger" disabled={bulkBusy} onclick={bulkDelete}>{t("Удалить")}</button>
      <span style="flex:1;"></span>
      <button class="btn-icon" title={t("Снять выбор")} onclick={clearSelection}>✕</button>
    </div>
  {/if}

  {#if aiError}
    <div class="ai-error">
      <span>{aiError}</span>
      <button class="btn-icon" style="color:white;" onclick={() => aiError = null}>✕</button>
    </div>
  {/if}

  {#if whatNow}
    <div class="what-now card">
      <span class="what-now-icon"><Icon name="target" size={16} /></span>
      <span class="what-now-text">{whatNow}</span>
      <button class="btn-icon" onclick={() => whatNow = null}>✕</button>
    </div>
  {/if}

  {#if viewMode === "board"}
    <div class="board">
      {#each statusStore.statuses.filter(s => s.id !== "Archived") as col (col.id)}
        <div
          class="column"
          role="list"
          class:drop-target={boardDropTargetStatus === col.id}
          ondragover={(e) => columnDragOver(e, col.id)}
          ondrop={(e) => columnDrop(e, col.id)}
          ondragleave={() => { if (boardDropTargetStatus === col.id) boardDropTargetStatus = null; }}
        >
          <div class="column-head">
            <span class="column-title" style="--cat: {col.color}">{statusStore.name(col.id)}</span>
            <span class="muted column-count">{boardTasksFor(col.id).length}</span>
            <button class="btn-icon" title={t("Новая задача")} onclick={() => openBoardCreate(col.id)}>+</button>
          </div>

          <div class="column-body">
            {#each boardTasksFor(col.id) as task (task.id)}
              <button
                class="board-card"
                class:dragging={boardDragTaskId === task.id}
                draggable="true"
                ondragstart={(e) => cardDragStart(e, task)}
                ondragend={() => { boardDragTaskId = null; boardDropTargetStatus = null; }}
                onclick={() => editingTask = task}
              >
                <div class="board-card-title">
                  <span class="prio-dot" style="--prio: var(--prio-{task.priority.toLowerCase()});" title="{t('Приоритет')}: {PRIORITY_LABELS[task.priority]}"></span>
                  {task.title}
                  {#if trackingId === task.id}
                    <span class="tracking-dot" title={t("Идёт трекинг")}><Icon name="play" size={10} /></span>
                  {/if}
                </div>
                <div class="board-card-meta">
                  <span class="chip chip-cat" style="--cat: {categoryStore.color(task.category)}">{categoryStore.name(task.category)}</span>
                  {#if task.deadline}
                    {@const dl = deadlineInfo(task.deadline)}
                    <span class="chip" class:chip-danger={dl.overdue}><Icon name="flag" size={10} /> {dl.label}</span>
                  {/if}
                  {#each task.tags as tag}
                    <span class="chip chip-tag">#{tag}</span>
                  {/each}
                </div>
              </button>
            {:else}
              <p class="empty-col muted">{t("Пусто")}</p>
            {/each}
          </div>
        </div>
      {/each}
      <div class="add-column">
        <button class="btn-sm" onclick={() => showStatusQuickAdd = true}>{t("+ Колонка")}</button>
        {#if showStatusQuickAdd}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            bind:value={newBoardStatusName}
            placeholder={t("Название статуса")}
            autofocus
            onkeydown={(e) => { if (e.key === "Enter") addBoardStatus(); if (e.key === "Escape") { showStatusQuickAdd = false; newBoardStatusName = ""; } }}
            onblur={() => { if (!newBoardStatusName.trim()) showStatusQuickAdd = false; }}
          />
        {/if}
      </div>
    </div>
  {:else}
  {#if listSubView === "active"}
  {#if todayBlocks.length > 0 && !searchQuery.trim()}
    <div class="day-plan card">
      <span class="day-plan-label">{t("Сегодня:")}</span>
      {#each todayBlocks as t (t.id)}
        <button class="chip day-plan-chip" onclick={() => editingTask = t} title={t.title}>
          <span class="day-plan-time">{blockTime(t)}</span> {t.title}
        </button>
      {/each}
    </div>
  {/if}

  {#if !searchQuery.trim()}
    <div class="smart-lists">
      <button
        class="chip smart-list-chip"
        class:active-toggle={activeSmartListId === null}
        onclick={() => activeSmartListId = null}
      >{t("Все")}</button>
      {#each BUILTIN_SMART_LISTS as l (l.id)}
        <button
          class="chip smart-list-chip"
          class:active-toggle={activeSmartListId === l.id}
          onclick={() => activeSmartListId = activeSmartListId === l.id ? null : l.id}
        >{t(l.name)}</button>
      {/each}
      {#each smartListStore.lists as l (l.id)}
        <span class="chip smart-list-chip custom" class:active-toggle={activeSmartListId === l.id}>
          <button class="smart-list-name" onclick={() => activeSmartListId = activeSmartListId === l.id ? null : l.id}>{l.name}</button>
          <button class="smart-list-remove" title={t("Удалить список")} onclick={() => removeSmartList(l.id)}>✕</button>
        </span>
      {/each}
      <button class="chip smart-list-chip smart-list-add" title={t("Создать умный список")} onclick={() => showSmartListModal = true}>{t("+ Список")}</button>
    </div>
  {/if}

  {#if !searchQuery.trim()}
    <div class="composer card">
      <textarea
        class="composer-input"
        bind:this={composerEl}
        bind:value={composerText}
        onkeydown={composerKeydown}
        rows={composerRows}
        placeholder={t("Быстрая задача… (!приоритет @категория #тег, завтра 15:00 — Shift+Enter подзадача, Ctrl+Enter создать)")}
      ></textarea>
      {#if composerDraft.title}
        <button class="btn-primary btn-sm composer-send" disabled={composerBusy} onclick={submitComposer}>
          {composerBusy ? "…" : t("Создать")}
        </button>
      {/if}
    </div>
    {#if composerDraft.title && (composerMeta.priority || composerMeta.categoryQuery || composerMeta.tags.length > 0 || composerMeta.deadline)}
      <div class="composer-preview">
        {#if composerMeta.priority}
          <span class="chip" style="--prio: var(--prio-{composerMeta.priority.toLowerCase()});">
            <span class="prio-dot"></span> {PRIORITY_LABELS[composerMeta.priority]}
          </span>
        {/if}
        {#if composerMeta.categoryQuery}
          {#if composerCategoryId}
            <span class="chip chip-cat" style="--cat: {categoryStore.color(composerCategoryId)}">{categoryStore.name(composerCategoryId)}</span>
          {:else}
            <span class="chip chip-danger" title={t("Категория «{q}» не найдена — будет «Другое»", { q: composerMeta.categoryQuery })}>@{composerMeta.categoryQuery} ?</span>
          {/if}
        {/if}
        {#each composerMeta.tags as tag}
          <span class="chip chip-tag">#{tag}</span>
        {/each}
        {#if composerMeta.deadline}
          <span class="chip"><Icon name="flag" size={11} /> {composerMeta.deadline.toLocaleString([], { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}</span>
        {/if}
      </div>
    {/if}
  {/if}

  {#if searchQuery.trim()}
    <div class="section-title">{t("Результаты поиска")}</div>
    {#if isSearching}
      <div class="empty">{t("Поиск…")}</div>
    {:else if searchResults.length === 0}
      <div class="empty">{t("Ничего не найдено")}</div>
    {:else}
      <ul class="task-list card">
        {#each searchResults as task (task.id)}
          {@render taskRow(task)}
        {/each}
      </ul>
    {/if}
  {:else}
    {#if taskStore.activeTasks.length === 0}
      <div class="empty card">
        {t("Нет активных задач.")}<br />
        <span class="muted">{t("Создайте первую: «+ Новая» или Ctrl+Shift+N")}</span>
      </div>
    {:else if filteredActive.length === 0}
      <div class="empty card">{activeSmartListId ? t("В этом списке нет задач") : t("В этом проекте нет активных задач")}</div>
    {:else if grouped}
      {#each grouped as group (group.id)}
        <div class="section-title project-head">
          <span>{group.name}</span>
          {#if group.total > 0}
            <span class="muted">{group.done}/{group.total}</span>
          {/if}
          {#if group.project}
            {@const goal = goalText(group.project)}
            {#if goal}
              <span class="goal-chip" class:met={goalMet(group.project)}
                title={group.project.goal_period === "month" ? t("Цель месяца") : t("Цель недели")}>
                {goal}
              </span>
            {/if}
          {/if}
        </div>
        <ul class="task-list card" style="margin-bottom:12px;">
          {#each group.tasks as task (task.id)}
            {@render taskRow(task)}
          {/each}
        </ul>
      {/each}
    {:else}
      <ul class="task-list card">
        {#each filteredActive as task (task.id)}
          {@render taskRow(task)}
        {/each}
      </ul>
    {/if}
  {/if}

  {:else if listSubView === "history"}
    <div class="empty-hint">
      {t("✓ Выполненные задачи. Повторяющиеся не попадают сюда — они остаются активными.")}
    </div>
    {#if taskStore.historyTasks.length === 0}
      <div class="empty card">{t("История пуста")}</div>
    {:else}
      <ul class="task-list card history">
        {#each taskStore.historyTasks as task (task.id)}
          <li class="task-row">
            <span class="task-check done history-icon">✓</span>
            <div
              class="task-main"
              onclick={() => historyDetailTask = task}
              onkeydown={(e) => { if (e.key === "Enter") historyDetailTask = task; }}
              role="button"
              tabindex="0"
            >
              <div class="task-title done-title">{task.title}</div>
              {#if task.description}
                <div class="task-desc">{task.description}</div>
              {/if}
            </div>
            <div class="task-meta">
              {#if task.subtasks.length > 0}
                <span class="chip">{doneCount(task)}/{task.subtasks.length}</span>
              {/if}
              <span class="chip">{statusStore.name(task.status)}</span>
            </div>
            <div class="task-actions">
              <button class="btn-icon btn-danger" title={t("Удалить")} onclick={() => taskStore.remove(task.id)}>✕</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

  {:else}
    <div class="empty-hint trash-hint">
      {t("🗑 Удалённые задачи. Восстановить можно в любой момент, пока не нажато «Удалить навсегда».")}
    </div>
    {#if taskStore.deletedTasks.length === 0}
      <div class="empty card">{t("Корзина пуста")}</div>
    {:else}
      <ul class="task-list card trash">
        {#each taskStore.deletedTasks as task (task.id)}
          <li class="task-row">
            <span class="task-check trash-icon">🗑</span>
            <div class="task-main">
              <div class="task-title done-title">{task.title}</div>
              {#if task.description}
                <div class="task-desc">{task.description}</div>
              {/if}
            </div>
            <div class="task-meta">
              {#if task.subtasks.length > 0}
                <span class="chip">{doneCount(task)}/{task.subtasks.length}</span>
              {/if}
            </div>
            <div class="task-actions">
              <button class="btn-sm" title={t("Восстановить")} onclick={() => taskStore.restore(task.id)}>{t("Восстановить")}</button>
              <button class="btn-icon btn-danger" title={t("Удалить навсегда")} onclick={() => taskStore.purge(task.id)}>✕</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
  {/if}
</div>

<style>
  .page {
    max-width: 860px;
    margin: 0 auto;
  }

  /* The board is wider than the list: several columns in a row do not fit into
     the narrow task-list container. */
  .page.board-mode {
    max-width: 1400px;
  }

  .page-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    flex-wrap: wrap;
    /* The window buttons float in the top right corner, so we reserve room for
       them or the search and filters end up underneath. The padding lives here
       rather than on .content: narrowing the whole column breaks views that
       compute their width in pixels (the graph clamps nodes with
       Math.min(width - 20)). */
    padding-right: var(--wincontrols-w);
  }

  .count { font-size: 12px; }

  /* .alert sets the background, colour and padding globally (app.css); only the
     layout for the close button is here. */
  .task-error {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .task-error span { flex: 1; }

  .head-search {
    width: 200px;
  }

  .active-toggle {
    background: var(--bg-hover);
    font-weight: 600;
  }

  .project-filter {
    max-width: 160px;
  }

  .project-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .proj-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .proj-row input {
    flex: 1;
    min-width: 0;
  }

  .proj-row.archived input {
    opacity: 0.55;
    text-decoration: line-through;
  }

  .proj-progress {
    font-size: 12px;
    flex-shrink: 0;
  }

  .proj-goal {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    margin: -4px 0 10px 8px;
    flex-wrap: wrap;
  }

  .proj-goal .goal-num {
    width: 58px;
    padding: 3px 6px;
    font-size: 12px;
  }

  .proj-goal select {
    padding: 3px 6px;
    font-size: 12px;
  }

  .goal-chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--bg-hover);
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .goal-chip.met {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    color: var(--success);
    font-weight: 600;
  }

  .goal-history {
    width: 100%;
    font-size: 11px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px 0 0 8px;
  }

  .goal-history-row {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .day-plan {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 12px;
    margin-bottom: 12px;
  }

  .task-row.dragging { opacity: 0.5; }
  .task-row.drop-target { box-shadow: inset 0 2px 0 var(--accent); }
  .task-row.selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .bulk-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 12px;
    margin-bottom: 12px;
  }

  .bulk-count {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }

  .composer {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 8px 12px;
    margin-bottom: 12px;
  }

  .composer-input {
    flex: 1;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    font-family: inherit;
    font-size: 13px;
    line-height: 1.5;
    padding: 2px 0;
  }
  .composer-input:focus { outline: none; }

  .composer-send { flex-shrink: 0; }

  .composer-preview {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 0 12px 10px;
    margin-top: -8px;
    margin-bottom: 12px;
  }

  .what-now {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    margin-bottom: 12px;
    border-left: 3px solid var(--accent);
    font-size: 13px;
  }

  .what-now-text { flex: 1; }

  .day-plan-label {
    font-size: 12px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .day-plan-chip {
    max-width: 260px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .day-plan-time {
    color: var(--accent);
    font-weight: 600;
  }

  .smart-lists {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }

  .smart-list-chip {
    cursor: pointer;
    border: none;
  }

  .smart-list-chip.custom {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding-right: 4px;
    cursor: default;
  }

  .smart-list-name {
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .smart-list-remove {
    border: none;
    background: transparent;
    padding: 0 2px;
    font-size: 10px;
    color: var(--text-secondary);
    cursor: pointer;
    line-height: 1;
  }

  .smart-list-remove:hover {
    color: var(--danger);
  }

  .smart-list-add {
    color: var(--text-secondary);
    background: transparent;
    border: 1px dashed var(--border);
  }

  .pair {
    display: flex;
    gap: 10px;
  }

  .pair .field {
    flex: 1;
  }

  .hint {
    font-size: 11px;
    color: var(--text-secondary);
    margin: 8px 0 0 0;
  }

  /* --- The board --- */
  .board {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    overflow-x: auto;
    padding-bottom: 8px;
  }

  .column {
    flex: 0 0 260px;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    border-radius: var(--radius);
    border: 1px solid var(--border);
    max-height: calc(100vh - 220px);
  }

  .column.drop-target {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .column-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }

  .column-title {
    font-weight: 600;
    font-size: 13px;
    color: var(--cat, var(--text-primary));
  }

  .column-count {
    font-size: 12px;
  }

  .column-head .btn-icon {
    margin-left: auto;
  }

  .column-body {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .empty-col {
    font-size: 12px;
    text-align: center;
    margin: 12px 0;
  }

  .board-card {
    display: block;
    width: 100%;
    text-align: left;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }

  .board-card:hover {
    background: var(--bg-hover);
  }

  .board-card.dragging {
    opacity: 0.5;
  }

  .board-card-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 6px;
  }

  .tracking-dot {
    margin-left: auto;
    color: var(--accent);
    display: inline-flex;
  }

  .board-card-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .add-column {
    flex: 0 0 180px;
  }

  .add-column input {
    width: 100%;
    margin-top: 4px;
  }

  .ai-error {
    background: var(--danger);
    color: white;
    padding: 6px 10px;
    border-radius: var(--radius);
    margin-bottom: 10px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .task-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  .task-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
  }

  .task-list > .task-row:last-child,
  .task-list > .task-sub-panel:last-child {
    border-bottom: none;
  }

  .task-row:hover {
    background: var(--bg-hover);
  }

  /* The round completion checkbox */
  .task-check {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    padding: 0;
    border-radius: 50%;
    border: 1.5px solid var(--text-secondary);
    background: transparent;
    color: transparent;
    font-size: 10px;
    line-height: 1;
  }

  .task-check:hover {
    border-color: var(--success);
    background: color-mix(in srgb, var(--success) 15%, transparent);
    color: var(--success);
  }

  .task-check.done {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-color: var(--success);
    color: var(--success);
    cursor: default;
  }

  .task-main {
    flex: 1;
    min-width: 0;
    cursor: pointer;
  }

  .task-title {
    font-size: 13px;
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .done-title {
    color: var(--text-secondary);
    text-decoration: line-through;
    font-weight: 400;
  }

  .prio-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--prio, var(--prio-low));
  }

  /* A blocked task is dimmed but readable: it stays in the list so it is not
     forgotten. Only the row's contents are dimmed, not the row itself — an
     opacity on .task-row would also mute the coloured priority bar on the
     left, which is what makes the list scannable. */
  .task-row.blocked .task-main,
  .task-row.blocked .task-meta { opacity: .55; }
  .task-row.blocked .task-check { cursor: not-allowed; }

  .task-blocked-by {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .task-desc {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 1px;
  }

  .task-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
  }

  .chip-sub {
    cursor: pointer;
    border: none;
    font-family: inherit;
  }
  .chip-sub:hover { background: var(--bg-hover); }

  /* A task WITH subtasks looks different from an empty "+": an accent chip with
     a mini progress bar, turning green once they are all done. */
  .chip-sub.has-subs {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    font-weight: 600;
  }
  .chip-sub.has-subs:hover { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  .chip-sub.subs-done {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 12%, transparent);
  }
  .chip-sub.subs-done:hover { background: color-mix(in srgb, var(--success) 20%, transparent); }

  .sub-track {
    width: 26px;
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, currentColor 25%, transparent);
    overflow: hidden;
  }
  .sub-fill {
    display: block;
    height: 100%;
    background: currentColor;
  }

  /* The actions are visible only on hovering the row */
  .task-actions {
    display: flex;
    gap: 1px;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity 0.12s;
  }

  .task-row:hover .task-actions {
    opacity: 1;
  }

  /* The subtasks panel / AI preview below the row */
  .task-sub-panel {
    list-style: none;
    padding: 6px 12px 8px 38px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .sub-preview-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }

  .sub-line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  /* The checklist rows moved into ChecklistEditor: striking through completed
     items and the field styling live there now. .sub-line remains — it is used
     by the AI suggestion rows above. */

  .history .task-row {
    opacity: 0.75;
  }

  /* The Trash uses the same muted row as History but with an explicit red accent
     on the icon, so "completed" and "deleted" are not confused visually (both
     used to share the same green .task-check.done). */
  .trash .task-row {
    opacity: 0.75;
  }

  .trash-icon {
    border-color: var(--danger) !important;
    color: var(--danger) !important;
  }

  .empty-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 10px;
  }
</style>
