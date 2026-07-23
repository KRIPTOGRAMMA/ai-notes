<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import { statusStore } from "../lib/stores/statuses.svelte";
  import { smartListStore } from "../lib/stores/smartLists.svelte";
  import { api } from "../lib/api/tauri";
  import { parseComposer, parseTaskText, matchCategoryQuery, SUBTASK_PREFIX } from "../lib/composer";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import TaskHistoryDetail from "../lib/components/TaskHistoryDetail.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import type { Task, Subtask, Category, CreateTaskPayload, UpdateTaskPayload, Project, GoalSnapshot, ActiveSession, SmartListFilter } from "../lib/types";

  type AiResult = { task_id: string; type: string; result?: string; error?: string };

  let showGoalHistory = $state<Record<string, GoalSnapshot[]>>({});
  let goalHistoryLoading = $state<Record<string, boolean>>({});

  // Список/История/Корзина — один взаимоисключающий переключатель (v0.9.22),
  // раньше были двумя независимыми тоглами (можно было открыть оба сразу,
  // визуально почти неотличимые друг от друга блоки под общим списком).
  let listSubView = $state<"active" | "history" | "trash">("active");
  let showCreateModal = $state(false);
  let editingTask: Task | null = $state(null);
  let historyDetailTask: Task | null = $state(null);

  // Список/Доска (v0.9.20) — переключатель в page-head, было отдельной
  // страницей Kanban.svelte, слито сюда, чтобы фильтры проекта/умного
  // списка/мультивыбор были общими для обоих режимов просмотра.
  let viewMode = $state<"list" | "board">("list");

  // Проекты: фильтр списка ("all" | "none" | id) и модал управления
  let projectFilter = $state<string>("all");
  let showProjects = $state(false);
  let newProjectName = $state("");

  // Умные списки: модалка создания своего списка
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
    // Капабилити-детект: при выключенном ИИ кнопка «Что сейчас?» просто скрыта
    api.getSettings().then(s => {
      aiEnabled = s.ai_provider !== "none";
      autoExpandSubs = s.show_subtasks_expanded;
    }).catch(() => {});
  });

  let aiEnabled = $state(false);
  // v0.8.3: задачи с подзадачами развёрнуты по умолчанию (настройка «Внешний вид»)
  let autoExpandSubs = $state(true);

  // Умные списки (v0.9.14): встроенные («Просроченные»/«На этой неделе») зависят
  // от текущей даты, поэтому целиком на фронте, в БД не хранятся; свои —
  // из smartListStore, предикат по category/priority/tag/наличию дедлайна.
  type BuiltinSmartList = { id: string; name: string; test: (t: Task) => boolean };
  const BUILTIN_SMART_LISTS: BuiltinSmartList[] = [
    {
      id: "__overdue",
      name: "Просроченные",
      test: (t) => !!t.deadline && new Date(t.deadline).getTime() < Date.now(),
    },
    {
      id: "__this_week",
      name: "На этой неделе",
      test: (t) => {
        if (!t.deadline) return false;
        const d = new Date(t.deadline).getTime();
        const now = Date.now();
        return d >= now && d <= now + 7 * 864e5;
      },
    },
  ];

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

  // Доска (v0.9.20): те же фильтры (проект/умный список), что и список, но
  // на базе taskStore.tasks, не activeTasks — выполненные задачи (hidden=true,
  // тот же флаг, что уводит их в Историю в режиме списка) должны остаться
  // видимыми в своей колонке на доске, а не пропадать со всей доски.
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

  // Мультивыбор не переживает смену видимого списка (фильтр/поиск/смена умного
  // списка) — иначе массовое действие могло бы незаметно задеть скрытые строки.
  $effect(() => {
    const visible = new Set(filteredActive.map(t => t.id));
    if ([...selectedIds].some(id => !visible.has(id))) {
      selectedIds = new Set([...selectedIds].filter(id => visible.has(id)));
    }
  });

  // Группировка «все проекты»: секция на проект (в порядке списка проектов) + «Без проекта».
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
      groups.push({ id: "", name: "Без проекта", done: 0, total: 0, tasks: orphan, project: null });
    }
    return groups.length > 0 ? groups : null;
  });

  // Цель проекта: текст прогресса «done/target задач · done/target мин» и её статус
  function goalText(p: Project): string | null {
    if (p.goal_tasks == null && p.goal_mins == null) return null;
    const parts: string[] = [];
    if (p.goal_tasks != null) parts.push(`${p.goal_done_tasks}/${p.goal_tasks} задач`);
    if (p.goal_mins != null) parts.push(`${p.goal_done_mins}/${p.goal_mins} мин`);
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

  // Расписание дня: сегодняшние тайм-блоки (назначаются в Календарь → Неделя)
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

  // --- Доска (v0.9.20): колонка на каждый статус из statusStore, а не
  // жёстко Todo/InProgress/Done — пользователь может добавлять свои. ---
  function boardTasksFor(statusId: string): Task[] {
    return boardTasks
      .filter(t => t.status === statusId)
      .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  }

  // Drag-and-drop: карточка → колонка (не карточка → карточка, как ручная
  // сортировка списка выше) — один dropzone на колонку, без ручного порядка
  // внутри неё (сортируем по updated_at).
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

  // InProgress/Done — особые случаи с side-эффектами (тайм-трекинг,
  // завершение), см. api.completeTask/startTaskTracking; остальные статусы
  // (включая пользовательские) — обычный update_task.
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

  // "+ Колонка" прямо на доске — быстрое добавление своего статуса без
  // перехода в Настройки (переименование/удаление остаются только там,
  // см. Settings.svelte «Статусы задач»).
  let showStatusQuickAdd = $state(false);
  let newBoardStatusName = $state("");

  async function addBoardStatus() {
    const name = newBoardStatusName.trim();
    if (!name) return;
    await statusStore.create(name, "#888888");
    newBoardStatusName = "";
    showStatusQuickAdd = false;
  }

  // Открытие задачи по сигналу извне (глобальный поиск Ctrl+K, попап дня в
  // Дашборде/Календаре). Завершённая задача (hidden) — это история: открываем
  // read-only TaskHistoryDetail, а не редактируемую TaskModal, иначе клик по
  // выполненной задаче из попапа дня открывал бы её как активную для правки
  // (дедлайн/повтор и т.п. уже не имеют смысла для того, что давно сделано).
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
    // Создание сразу в InProgress (например, через "+ колонка" на доске) —
    // статус уже проставлен модалкой (initialStatus), но реальный
    // трекинг-таймер запускается отдельным вызовом, как и везде в приложении.
    if (created && payload.status === "InProgress") {
      await api.startTaskTracking(created.id);
      trackingId = created.id;
      await taskStore.load();
    }
    return created;
  }

  // --- Инлайн-композер: первая строка — название, Enter — перенос,
  // Shift+Enter — строка-подзадача (☐), Ctrl+Enter — создать. ---
  let composerText = $state("");
  let composerEl: HTMLTextAreaElement | undefined = $state();
  let composerBusy = $state(false);
  const composerRows = $derived(Math.min(6, composerText.split("\n").length));

  // Естественный язык в названии (v0.9.17): !приоритет / @категория / #тег /
  // относительные даты-время разбираются из первой строки живьём, по мере ввода.
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
    // обычный Enter — дефолтный перенос строки
  }

  async function submitComposer() {
    const draft = parseComposer(composerText);
    if (!draft.title || composerBusy) return;
    const meta = parseTaskText(draft.title);
    composerBusy = true;
    try {
      // Активный фильтр проекта — умный дефолт для новой задачи
      const projectId = projectFilter !== "all" && projectFilter !== "none" ? projectFilter : null;
      const categoryId = meta.categoryQuery ? matchCategoryQuery(categoryStore.categories, meta.categoryQuery) : null;
      const task = await api.createTask({
        title: meta.title || draft.title,
        description: draft.description || null,
        status: "Todo",
        priority: meta.priority ?? "Medium",
        category: categoryId ?? "Other", // фолбэк-категория: всегда существует (Work можно удалить)
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
      aiError = typeof e === "string" ? e : "Не удалось создать задачу";
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

  // Добавить одну AI-подзадачу как чек-лист-пункт под родительскую задачу
  async function acceptSubtask(parentId: string, title: string) {
    await api.addSubtask(parentId, title);
    await taskStore.load();
  }

  // Принять все предложенные подзадачи разом
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

  // --- Инлайн-чеклист в панели строки (v0.8.3, стиль Xiaomi Notes):
  // существующие подзадачи редактируются на месте (Enter/blur — commit,
  // Backspace на пустой — удалить), последняя строка — драфт новой.
  let subDraft = $state<Record<string, string>>({});
  let draftEls = $state<Record<string, HTMLInputElement>>({});

  async function commitDraft(taskId: string, refocus = true) {
    const title = (subDraft[taskId] ?? "").trim();
    if (!title) return;
    await api.addSubtask(taskId, title);
    subDraft[taskId] = "";
    await taskStore.load();
    if (refocus) { await tick(); draftEls[taskId]?.focus(); }
  }

  async function commitRename(sub: Subtask, value: string) {
    const t = value.trim();
    if (!t) { // очистил и ушёл — считаем удалением (симметрично Backspace)
      await api.deleteSubtask(sub.id);
      await taskStore.load();
      return;
    }
    if (t !== sub.title) {
      await api.renameSubtask(sub.id, t);
      await taskStore.load();
    }
  }

  async function onSubRowKeydown(e: KeyboardEvent, taskId: string, sub: Subtask) {
    const input = e.currentTarget as HTMLInputElement;
    if (e.key === "Enter") {
      e.preventDefault();
      await commitRename(sub, input.value);
      await tick();
      draftEls[taskId]?.focus();
    } else if (e.key === "Backspace" && input.value === "") {
      e.preventDefault();
      await api.deleteSubtask(sub.id);
      await taskStore.load();
      await tick();
      draftEls[taskId]?.focus();
    }
  }

  function onDraftKeydown(e: KeyboardEvent, taskId: string) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitDraft(taskId);
    }
  }

  let expanded = $state<Record<string, boolean>>({});

  // Явный клик переопределяет авто-разворачивание; без клика — задачи с
  // подзадачами открыты, если включена настройка show_subtasks_expanded.
  function isExpanded(task: Task): boolean {
    return expanded[task.id] ?? (autoExpandSubs && task.subtasks.length > 0);
  }

  // --- Ручная сортировка: drag строки в пределах своего списка (группы) ---
  let dragTaskId: string | null = $state(null);
  let dropTargetId: string | null = $state(null);

  // --- Мультивыбор (v0.9.15): Ctrl/Shift+клик по строке вместо открытия карточки.
  // Ctrl — точечный тоггл, Shift — диапазон от последней выбранной строки в
  // пределах текущего видимого списка (без учёта группировки — «плоский» порядок).
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
    if (from < 0 || to < 0) return; // перетаскивание между группами — не сортировка
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

  const PRIORITY_LABELS: Record<string, string> = {
    Low: "Низкий", Medium: "Средний", High: "Высокий", Critical: "Критический",
  };

  function recurrenceLabel(r: unknown): string | null {
    if (!r || r === "None") return null;
    if (r === "Hourly") return "Каждый час";
    if (r === "Daily")  return "Каждый день";
    if (r === "Weekly") return "Каждую неделю";
    if (typeof r === "object" && r !== null && "Custom" in r) {
      const [n, unit] = (r as any).Custom;
      const unitLabel =
        unit === "Minutes" ? "мин." :
        unit === "Hours"   ? "ч." :
        unit === "Days"    ? "дн." : "нед.";
      return `раз в ${n} ${unitLabel}`;
    }
    if (typeof r === "object" && r !== null && "Weekdays" in r) {
      const labels = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
      const mask = (r as any).Weekdays as number;
      const days = labels.filter((_, i) => mask & (1 << i));
      return `по ${days.join(", ")}`;
    }
    return null;
  }

  // Компактный дедлайн: «сегодня 18:00», «завтра», «3 дн», «просрочено 2 дн»
  function deadlineInfo(iso: string): { label: string; overdue: boolean } {
    const d = new Date(iso);
    const now = new Date();
    const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
    const dayDiff = Math.round((startOfDay(d) - startOfDay(now)) / 864e5);

    if (d.getTime() < now.getTime()) {
      return { label: dayDiff === 0 ? "просрочено" : `просрочено ${-dayDiff} дн`, overdue: true };
    }
    if (dayDiff === 0) {
      return { label: `сегодня ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`, overdue: false };
    }
    if (dayDiff === 1) return { label: "завтра", overdue: false };
    if (dayDiff < 7) return { label: `${dayDiff} дн`, overdue: false };
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

  // «Что делать сейчас»: совет ИИ по текущему контексту (блоки, дедлайны, приоритеты)
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
  <li
    class="task-row"
    style="--prio: var(--prio-{task.priority.toLowerCase()});"
    class:dragging={dragTaskId === task.id}
    class:drop-target={dropTargetId === task.id}
    class:selected={selectedIds.has(task.id)}
    draggable={!searchQuery.trim() && !task.hidden}
    ondragstart={(e) => rowDragStart(e, task)}
    ondragover={(e) => rowDragOver(e, task)}
    ondrop={(e) => rowDrop(e, task)}
    ondragend={() => { dragTaskId = null; dropTargetId = null; }}
  >
    <button
      class="task-check"
      onclick={async () => { await taskStore.complete(task.id); projectStore.load(); }}
      title="Выполнить"
      aria-label="Выполнить задачу"
    ></button>

    <div
      class="task-main"
      onclick={(e) => onRowClick(e, task)}
      onkeydown={(e) => { if (e.key === "Enter") editingTask = task; }}
      role="button"
      tabindex="0"
    >
      <div class="task-title">
        <span class="prio-dot" title="Приоритет: {PRIORITY_LABELS[task.priority]}"></span>
        {task.title}
        {#if recurrenceLabel(task.recurrence)}
          <span class="muted" title={recurrenceLabel(task.recurrence)}>↻</span>
        {/if}
      </div>
      {#if task.description}
        <div class="task-desc">{task.description}</div>
      {/if}
    </div>

    <div class="task-meta">
      <button
        class="chip chip-sub"
        class:has-subs={task.subtasks.length > 0}
        class:subs-done={task.subtasks.length > 0 && doneCount(task) === task.subtasks.length}
        onclick={() => expanded[task.id] = !isExpanded(task)}
        title={task.subtasks.length > 0 ? "Подзадачи" : "Добавить подзадачу"}
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
      <button class="btn-icon" disabled={busy} title="Переформулировать в SMART"
        onclick={() => rewriteTask(task.id, task.title)}>{#if busy}…{:else}<Icon name="sparkles" />{/if}</button>
      <button class="btn-icon" disabled={busy} title="Разбить на подзадачи"
        onclick={() => generateSubtasks(task.id, task.title)}>{#if busy}…{:else}<Icon name="shuffle" />{/if}</button>
      <button class="btn-icon" disabled={busy} title="Авто-категория"
        onclick={() => classifyTask(task.id, task.title)}>{#if busy}…{:else}<Icon name="tag" />{/if}</button>
      <button class="btn-icon" title={trackingId === task.id ? "Остановить трекинг" : "Начать трекинг"}
        onclick={() => toggleTracking(task.id)} class:active={trackingId === task.id}>
        {#if trackingId === task.id}<Icon name="stop" />{:else}<Icon name="play" />{/if}</button>
      <button class="btn-icon btn-danger" title="Удалить"
        onclick={() => taskStore.remove(task.id)}>✕</button>
    </div>
  </li>

  {#if subtasksPreview && subtasksPreview.taskId === task.id}
    <li class="task-sub-panel">
      <div class="sub-preview-head">
        <span class="section-title" style="margin:0;">ИИ предлагает подзадачи</span>
        <div style="display:flex;gap:6px;">
          <button class="btn-sm btn-primary" onclick={() => acceptAllSubtasks(task.id, subtasksPreview!.items)}>Принять все</button>
          <button class="btn-sm" onclick={() => subtasksPreview = null}>Закрыть</button>
        </div>
      </div>
      {#each subtasksPreview.items as subtask}
        <div class="sub-line">
          <span style="flex:1;">{subtask}</span>
          <button class="btn-sm" onclick={() => acceptSubtask(task.id, subtask)}>+ Добавить</button>
        </div>
      {/each}
    </li>
  {/if}

  {#if isExpanded(task)}
    <li class="task-sub-panel">
      {#each task.subtasks as sub (sub.id)}
        <div class="sub-line">
          <input type="checkbox" checked={sub.done} onchange={() => toggleSubtask(sub.id)} />
          <input
            class="check-input"
            class:sub-done={sub.done}
            value={sub.title}
            onblur={(e) => commitRename(sub, e.currentTarget.value)}
            onkeydown={(e) => onSubRowKeydown(e, task.id, sub)}
          />
        </div>
      {/each}
      <div class="sub-line">
        <input
          class="check-input"
          placeholder="+ подзадача (Enter)"
          bind:value={subDraft[task.id]}
          bind:this={draftEls[task.id]}
          onkeydown={(e) => onDraftKeydown(e, task.id)}
          onblur={() => commitDraft(task.id, false)}
        />
      </div>
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
      <h2 class="dialog-title">Проекты</h2>

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
          <button class="btn-sm" title={p.archived ? "Разархивировать" : "В архив"}
            onclick={() => projectStore.update(p.id, { archived: !p.archived })}>
            {p.archived ? "Вернуть" : "Архив"}
          </button>
          <button class="btn-icon btn-danger" title="Удалить проект (задачи останутся без проекта)"
            onclick={() => projectStore.remove(p.id)}>✕</button>
        </div>
        {#if !p.archived}
          <div class="proj-goal">
            <span class="muted">Цель:</span>
            <input class="goal-num" type="number" min="0" placeholder="—"
              value={p.goal_tasks ?? ""}
              onchange={(e) => projectStore.update(p.id, { goal_tasks: Number(e.currentTarget.value) || 0 })}
            />
            <span class="muted">задач ·</span>
            <input class="goal-num" type="number" min="0" step="15" placeholder="—"
              value={p.goal_mins ?? ""}
              onchange={(e) => projectStore.update(p.id, { goal_mins: Number(e.currentTarget.value) || 0 })}
            />
            <span class="muted">мин в</span>
            <select
              value={p.goal_period}
              onchange={(e) => projectStore.update(p.id, { goal_period: e.currentTarget.value as "week" | "month" })}
            >
              <option value="week">неделю</option>
              <option value="month">месяц</option>
            </select>
            {#if goalText(p)}
              <span class="goal-chip" class:met={goalMet(p)}>{goalText(p)}</span>
              <button class="btn-sm" onclick={() => toggleGoalHistory(p.id)}>
                {showGoalHistory[p.id] ? "Скрыть" : "История"}
              </button>
            {/if}
            {#if showGoalHistory[p.id]}
              <div class="goal-history">
                {#if goalHistoryLoading[p.id]}
                  <span class="muted">Загрузка…</span>
                {:else if showGoalHistory[p.id].length === 0}
                  <span class="muted">Нет записей</span>
                {:else}
                  {#each showGoalHistory[p.id] as snap (snap.id)}
                    <div class="goal-history-row">
                      <span class="muted">{snap.recorded_at.slice(0, 16)}</span>
                      <span>{snap.done_tasks}{snap.goal_tasks != null ? `/${snap.goal_tasks}` : ''} задач</span>
                      <span>·</span>
                      <span>{snap.done_mins}{snap.goal_mins != null ? `/${snap.goal_mins}` : ''} мин</span>
                    </div>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      {:else}
        <p class="muted" style="margin:0;font-size:13px;">Проектов пока нет — создайте первый.</p>
      {/each}

      <div class="proj-row">
        <input
          bind:value={newProjectName}
          placeholder="Название нового проекта"
          onkeydown={(e) => { if (e.key === "Enter") addProject(); }}
        />
        <button class="btn-primary" onclick={addProject} disabled={!newProjectName.trim()}>Создать</button>
      </div>

      <div class="actions">
        <button class="btn-ghost" onclick={() => showProjects = false}>Закрыть</button>
      </div>
    </div>
  </div>
{/if}

{#if showSmartListModal}
  <div role="dialog" aria-modal="true" class="overlay backdrop"
    onclick={(e) => { if (e.target === e.currentTarget) { showSmartListModal = false; resetSmartListForm(); } }}>
    <div class="modal dialog">
      <h2 class="dialog-title">Новый умный список</h2>

      {#if smartListStore.error}
        <div class="alert" style="margin:0;">{smartListStore.error}</div>
      {/if}

      <label class="field">
        <span class="label">Название</span>
        <input bind:value={newSmartListName} placeholder="Например: Важное" />
      </label>

      <div class="pair" style="margin-top:8px;">
        <label class="field">
          <span class="label">Категория</span>
          <select bind:value={newSmartListCategory}>
            <option value="">Любая</option>
            {#each categoryStore.categories as c (c.id)}
              <option value={c.id}>{c.name}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span class="label">Приоритет</span>
          <select bind:value={newSmartListPriority}>
            <option value="">Любой</option>
            {#each Object.entries(PRIORITY_LABELS) as [value, label] (value)}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>
      </div>

      <div class="pair" style="margin-top:8px;">
        <label class="field">
          <span class="label">Тег</span>
          <input bind:value={newSmartListTag} placeholder="без #" />
        </label>
        <label class="field">
          <span class="label">Дедлайн</span>
          <select bind:value={newSmartListHasDeadline}>
            <option value="">Не важно</option>
            <option value="yes">Есть дедлайн</option>
            <option value="no">Без дедлайна</option>
          </select>
        </label>
      </div>

      <p class="hint">Условия комбинируются через «И» — задача должна подойти под все заданные.</p>

      <div class="actions">
        <button class="btn-ghost" onclick={() => { showSmartListModal = false; resetSmartListForm(); }}>Отмена</button>
        <button class="btn-primary" onclick={createSmartList} disabled={!newSmartListName.trim()}>Создать</button>
      </div>
    </div>
  </div>
{/if}

<div class="page" class:board-mode={viewMode === "board"}>
  <div class="page-head">
    <h1 class="page-title">Задачи</h1>
    <span class="muted count">
      {taskStore.activeTasks.length} актив. · {taskStore.historyTasks.length} в истории
    </span>
    <div class="seg">
      <button class:active={viewMode === "list"} onclick={() => viewMode = "list"}>Список</button>
      <button class:active={viewMode === "board"} onclick={() => viewMode = "board"}>Доска</button>
    </div>
    <span style="flex:1;"></span>
    <input
      bind:value={searchQuery}
      oninput={handleSearch}
      placeholder="Поиск задач…"
      class="head-search"
    />
    {#if projectStore.projects.length > 0}
      <select bind:value={projectFilter} class="project-filter" title="Фильтр по проекту">
        <option value="all">Все проекты</option>
        <option value="none">Без проекта</option>
        {#each projectStore.active as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
    {/if}
    {#if aiEnabled}
      <button onclick={askWhatNow} disabled={whatNowPending}
        title="ИИ посоветует, чем заняться сейчас — по блокам, дедлайнам и приоритетам">
        {#if whatNowPending}Думаю…{:else}<Icon name="target" size={12} /> Что сейчас?{/if}
      </button>
    {/if}
    <button onclick={() => { showProjects = true; projectStore.load(); }}>Проекты</button>
    <div class="seg">
      <button class:active={listSubView === "active"} onclick={() => listSubView = "active"}>Активные</button>
      <button class:active={listSubView === "history"} onclick={() => listSubView = "history"}>История</button>
      <button class:active={listSubView === "trash"} onclick={() => { listSubView = "trash"; taskStore.loadDeleted(); }}>Корзина</button>
    </div>
    <button class="btn-primary" onclick={() => { boardCreateStatus = "Todo"; showCreateModal = true; }}>+ Новая</button>
  </div>

  {#if selectedIds.size > 0}
    <div class="bulk-bar card">
      <span class="bulk-count">{selectedIds.size} выбрано</span>
      <select bind:value={bulkProjectId} disabled={bulkBusy} title="Перенести в проект">
        <option value="" disabled selected>В проект…</option>
        <option value="none">Без проекта</option>
        {#each projectStore.active as p (p.id)}
          <option value={p.id}>{p.name}</option>
        {/each}
      </select>
      {#if bulkProjectId}
        <button class="btn-sm" disabled={bulkBusy} onclick={bulkMoveToProject}>Перенести</button>
      {/if}
      <select bind:value={bulkCategory} disabled={bulkBusy} title="Сменить категорию">
        <option value="" disabled selected>Категория…</option>
        {#each categoryStore.categories as c (c.id)}
          <option value={c.id}>{c.name}</option>
        {/each}
      </select>
      {#if bulkCategory}
        <button class="btn-sm" disabled={bulkBusy} onclick={bulkSetCategory}>Применить</button>
      {/if}
      <button class="btn-sm" disabled={bulkBusy} onclick={bulkComplete}>Выполнить</button>
      <button class="btn-sm btn-danger" disabled={bulkBusy} onclick={bulkDelete}>Удалить</button>
      <span style="flex:1;"></span>
      <button class="btn-icon" title="Снять выбор" onclick={clearSelection}>✕</button>
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
            <span class="column-title" style="--cat: {col.color}">{col.name}</span>
            <span class="muted column-count">{boardTasksFor(col.id).length}</span>
            <button class="btn-icon" title="Новая задача" onclick={() => openBoardCreate(col.id)}>+</button>
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
                  <span class="prio-dot" style="--prio: var(--prio-{task.priority.toLowerCase()});" title="Приоритет: {PRIORITY_LABELS[task.priority]}"></span>
                  {task.title}
                  {#if trackingId === task.id}
                    <span class="tracking-dot" title="Идёт трекинг"><Icon name="play" size={10} /></span>
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
              <p class="empty-col muted">Пусто</p>
            {/each}
          </div>
        </div>
      {/each}
      <div class="add-column">
        <button class="btn-sm" onclick={() => showStatusQuickAdd = true}>+ Колонка</button>
        {#if showStatusQuickAdd}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            bind:value={newBoardStatusName}
            placeholder="Название статуса"
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
      <span class="day-plan-label">Сегодня:</span>
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
      >Все</button>
      {#each BUILTIN_SMART_LISTS as l (l.id)}
        <button
          class="chip smart-list-chip"
          class:active-toggle={activeSmartListId === l.id}
          onclick={() => activeSmartListId = activeSmartListId === l.id ? null : l.id}
        >{l.name}</button>
      {/each}
      {#each smartListStore.lists as l (l.id)}
        <span class="chip smart-list-chip custom" class:active-toggle={activeSmartListId === l.id}>
          <button class="smart-list-name" onclick={() => activeSmartListId = activeSmartListId === l.id ? null : l.id}>{l.name}</button>
          <button class="smart-list-remove" title="Удалить список" onclick={() => removeSmartList(l.id)}>✕</button>
        </span>
      {/each}
      <button class="chip smart-list-chip smart-list-add" title="Создать умный список" onclick={() => showSmartListModal = true}>+ Список</button>
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
        placeholder="Быстрая задача… (!приоритет @категория #тег, завтра 15:00 — Shift+Enter подзадача, Ctrl+Enter создать)"
      ></textarea>
      {#if composerDraft.title}
        <button class="btn-primary btn-sm composer-send" disabled={composerBusy} onclick={submitComposer}>
          {composerBusy ? "…" : "Создать"}
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
            <span class="chip chip-danger" title="Категория «{composerMeta.categoryQuery}» не найдена — будет «Другое»">@{composerMeta.categoryQuery} ?</span>
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
    <div class="section-title">Результаты поиска</div>
    {#if isSearching}
      <div class="empty">Поиск…</div>
    {:else if searchResults.length === 0}
      <div class="empty">Ничего не найдено</div>
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
        Нет активных задач.<br />
        <span class="muted">Создайте первую: «+ Новая» или Ctrl+Shift+N</span>
      </div>
    {:else if filteredActive.length === 0}
      <div class="empty card">{activeSmartListId ? "В этом списке нет задач" : "В этом проекте нет активных задач"}</div>
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
                title={group.project.goal_period === "month" ? "Цель месяца" : "Цель недели"}>
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
      ✓ Выполненные задачи. Повторяющиеся не попадают сюда — они остаются активными.
    </div>
    {#if taskStore.historyTasks.length === 0}
      <div class="empty card">История пуста</div>
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
              <button class="btn-icon btn-danger" title="Удалить" onclick={() => taskStore.remove(task.id)}>✕</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}

  {:else}
    <div class="empty-hint trash-hint">
      🗑 Удалённые задачи. Восстановить можно в любой момент, пока не нажато «Удалить навсегда».
    </div>
    {#if taskStore.deletedTasks.length === 0}
      <div class="empty card">Корзина пуста</div>
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
              <button class="btn-sm" title="Восстановить" onclick={() => taskStore.restore(task.id)}>Восстановить</button>
              <button class="btn-icon btn-danger" title="Удалить навсегда" onclick={() => taskStore.purge(task.id)}>✕</button>
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

  /* Доска (v0.9.20) шире списка — несколько колонок в ряд не помещаются
     в узкий контейнер списка задач. */
  .page.board-mode {
    max-width: 1400px;
  }

  .page-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }

  .count { font-size: 12px; }

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

  /* --- Доска (v0.9.20) --- */
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

  /* Круглый чекбокс выполнения */
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

  /* Задача С подзадачами визуально отличается от пустого «+» (v0.8.2):
     акцентный чип с мини-прогрессом, зелёный — когда все выполнены. */
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

  /* Действия видны только при наведении на строку */
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

  /* Панель подзадач / ИИ-превью под строкой */
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

  .sub-done {
    text-decoration: line-through;
    color: var(--text-secondary);
  }

  /* Инлайн-чеклист панели (v0.8.3): borderless-строки, как в модалке */
  .check-input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 3px 4px;
    font-size: 13px;
    border-bottom: 1px solid transparent;
    border-radius: 0;
  }
  .check-input:focus {
    outline: none;
    border-bottom-color: var(--accent);
  }

  .history .task-row {
    opacity: 0.75;
  }

  /* Корзина (v0.9.22) — тот же приглушённый ряд, что История, но с явным
     красным акцентом на иконке, чтобы «выполнено» и «удалено» не путались
     визуально (раньше оба использовали одинаковый зелёный .task-check.done). */
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
