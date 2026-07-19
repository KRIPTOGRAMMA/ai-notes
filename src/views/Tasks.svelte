<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import { api } from "../lib/api/tauri";
  import { parseComposer, SUBTASK_PREFIX } from "../lib/composer";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import type { Task, Category, CreateTaskPayload, UpdateTaskPayload, Project, GoalSnapshot, ActiveSession, ChecklistTemplate } from "../lib/types";

  type AiResult = { task_id: string; type: string; result?: string; error?: string };

  let showGoalHistory = $state<Record<string, GoalSnapshot[]>>({});
  let goalHistoryLoading = $state<Record<string, boolean>>({});

  let showHistory = $state(false);
  let showCreateModal = $state(false);
  let editingTask: Task | null = $state(null);

  // Проекты: фильтр списка ("all" | "none" | id) и модал управления
  let projectFilter = $state<string>("all");
  let showProjects = $state(false);
  let newProjectName = $state("");

  onMount(() => {
    projectStore.load();
    categoryStore.load();
    // Капабилити-детект: при выключенном ИИ кнопка «Что сейчас?» просто скрыта
    api.getSettings().then(s => aiEnabled = s.ai_provider !== "none").catch(() => {});
  });

  let aiEnabled = $state(false);

  const filteredActive = $derived(
    taskStore.activeTasks.filter(t =>
      projectFilter === "all" ? true :
      projectFilter === "none" ? !t.project_id :
      t.project_id === projectFilter
    )
  );

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

  // Открытие задачи по сигналу из глобального поиска (Ctrl+K).
  $effect(() => {
    const id = taskStore.focusTaskId;
    if (!id) return;
    const task = taskStore.tasks.find(t => t.id === id);
    if (task) editingTask = task;
    taskStore.clearFocus();
  });

  async function handleCreate(data: CreateTaskPayload | UpdateTaskPayload) {
    await taskStore.create(data as CreateTaskPayload);
  }

  // --- Инлайн-композер: первая строка — название, Enter — перенос,
  // Shift+Enter — строка-подзадача (☐), Ctrl+Enter — создать. ---
  let composerText = $state("");
  let composerEl: HTMLTextAreaElement | undefined = $state();
  let composerBusy = $state(false);
  const composerRows = $derived(Math.min(6, composerText.split("\n").length));

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
    composerBusy = true;
    try {
      // Активный фильтр проекта — умный дефолт для новой задачи
      const projectId = projectFilter !== "all" && projectFilter !== "none" ? projectFilter : null;
      const task = await api.createTask({
        title: draft.title,
        description: draft.description || null,
        status: "Todo",
        priority: "Medium",
        category: "Other", // фолбэк-категория: всегда существует (Work можно удалить)
        deadline: null,
        tags: [],
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

  async function removeSubtask(id: string) {
    await api.deleteSubtask(id);
    await taskStore.load();
  }

  let newSubtaskInput = $state<Record<string, string>>({});
  async function addManualSubtask(parentId: string) {
    const title = (newSubtaskInput[parentId] ?? "").trim();
    if (!title) return;
    await api.addSubtask(parentId, title);
    newSubtaskInput[parentId] = "";
    await taskStore.load();
  }

  // --- Шаблоны чеклистов (v0.7.15) ---
  let checklistTemplates: ChecklistTemplate[] = $state([]);
  let templatePickerFor: string | null = $state(null); // task_id, для которой открыт список «Из шаблона…»
  let savingTemplateFor: string | null = $state(null); // task_id, для которой открыт инпут «Сохранить как шаблон»
  let newTemplateName = $state("");

  async function loadChecklistTemplates() {
    checklistTemplates = await api.getChecklistTemplates().catch(() => []);
  }

  function openTemplatePicker(taskId: string) {
    templatePickerFor = templatePickerFor === taskId ? null : taskId;
    savingTemplateFor = null;
    if (templatePickerFor) loadChecklistTemplates();
  }

  async function applyTemplate(taskId: string, template: ChecklistTemplate) {
    for (const item of template.items) {
      await api.addSubtask(taskId, item);
    }
    templatePickerFor = null;
    await taskStore.load();
  }

  async function removeTemplate(id: string) {
    await api.deleteChecklistTemplate(id);
    await loadChecklistTemplates();
  }

  function openSaveTemplate(taskId: string) {
    savingTemplateFor = savingTemplateFor === taskId ? null : taskId;
    templatePickerFor = null;
    newTemplateName = "";
  }

  async function saveCurrentAsTemplate(task: Task) {
    const name = newTemplateName.trim();
    if (!name || task.subtasks.length === 0) return;
    await api.createChecklistTemplate(name, task.subtasks.map(s => s.title));
    savingTemplateFor = null;
    newTemplateName = "";
  }

  let expanded = $state<Record<string, boolean>>({});

  // --- Ручная сортировка: drag строки в пределах своего списка (группы) ---
  let dragTaskId: string | null = $state(null);
  let dropTargetId: string | null = $state(null);

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
      onclick={() => editingTask = task}
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
        onclick={() => expanded[task.id] = !expanded[task.id]}
        title={task.subtasks.length > 0 ? "Подзадачи" : "Добавить подзадачу"}
      >{expanded[task.id] ? "▾" : "▸"} {task.subtasks.length > 0 ? `${doneCount(task)}/${task.subtasks.length}` : "+"}</button>
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

  {#if expanded[task.id]}
    <li class="task-sub-panel">
      {#each task.subtasks as sub (sub.id)}
        <div class="sub-line">
          <input type="checkbox" checked={sub.done} onchange={() => toggleSubtask(sub.id)} />
          <span style="flex:1;" class:sub-done={sub.done}>{sub.title}</span>
          <button class="btn-icon btn-danger" title="Удалить" onclick={() => removeSubtask(sub.id)}>✕</button>
        </div>
      {/each}
      <div class="sub-line">
        <input
          type="text"
          placeholder="+ подзадача"
          bind:value={newSubtaskInput[task.id]}
          onkeydown={(e) => { if (e.key === 'Enter') addManualSubtask(task.id); }}
          class="sub-input"
        />
        <button class="btn-sm" onclick={() => addManualSubtask(task.id)}>Добавить</button>
      </div>
      <div class="sub-line" style="gap:6px;">
        <button class="btn-sm" onclick={() => openTemplatePicker(task.id)}>Из шаблона…</button>
        <button class="btn-sm" onclick={() => openSaveTemplate(task.id)} disabled={task.subtasks.length === 0}
          title={task.subtasks.length === 0 ? "Сначала добавьте подзадачи" : ""}>
          Сохранить как шаблон
        </button>
      </div>

      {#if templatePickerFor === task.id}
        <div class="template-panel">
          {#if checklistTemplates.length === 0}
            <span class="muted" style="font-size:12px;">Нет сохранённых шаблонов</span>
          {:else}
            {#each checklistTemplates as tpl (tpl.id)}
              <div class="sub-line">
                <span style="flex:1;">{tpl.name} <span class="muted">({tpl.items.length})</span></span>
                <button class="btn-sm" onclick={() => applyTemplate(task.id, tpl)}>Применить</button>
                <button class="btn-icon btn-danger" title="Удалить шаблон" onclick={() => removeTemplate(tpl.id)}>✕</button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      {#if savingTemplateFor === task.id}
        <div class="sub-line template-panel">
          <input
            type="text"
            placeholder="Название шаблона"
            bind:value={newTemplateName}
            onkeydown={(e) => { if (e.key === 'Enter') saveCurrentAsTemplate(task); }}
            class="sub-input"
          />
          <button class="btn-sm btn-primary" onclick={() => saveCurrentAsTemplate(task)} disabled={!newTemplateName.trim()}>
            Сохранить
          </button>
        </div>
      {/if}
    </li>
  {/if}
{/snippet}

<!-- Modals -->
{#if showCreateModal}
  <TaskModal
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

<div class="page">
  <div class="page-head">
    <h1 class="page-title">Задачи</h1>
    <span class="muted count">
      {taskStore.activeTasks.length} актив. · {taskStore.historyTasks.length} в истории
    </span>
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
    <button class:active-toggle={showHistory} onclick={() => showHistory = !showHistory}>История</button>
    <button class="btn-primary" onclick={() => showCreateModal = true}>+ Новая</button>
  </div>

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
    <div class="composer card">
      <textarea
        class="composer-input"
        bind:this={composerEl}
        bind:value={composerText}
        onkeydown={composerKeydown}
        rows={composerRows}
        placeholder="Быстрая задача…  (Shift+Enter — подзадача, Ctrl+Enter — создать)"
      ></textarea>
      {#if parseComposer(composerText).title}
        <button class="btn-primary btn-sm composer-send" disabled={composerBusy} onclick={submitComposer}>
          {composerBusy ? "…" : "Создать"}
        </button>
      {/if}
    </div>
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
      <div class="empty card">В этом проекте нет активных задач</div>
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

    {#if showHistory}
      <div class="section-title" style="margin-top:20px;">История</div>
      {#if taskStore.historyTasks.length === 0}
        <div class="empty">История пуста</div>
      {:else}
        <ul class="task-list card history">
          {#each taskStore.historyTasks as task (task.id)}
            <li class="task-row">
              <span class="task-check done">✓</span>
              <div class="task-main">
                <div class="task-title done-title">{task.title}</div>
                {#if task.description}
                  <div class="task-desc">{task.description}</div>
                {/if}
              </div>
              <div class="task-meta">
                <span class="chip">{task.status === "Done" ? "Выполнена" : task.status}</span>
              </div>
              <div class="task-actions">
                <button class="btn-icon btn-danger" title="Удалить" onclick={() => taskStore.remove(task.id)}>✕</button>
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

  .sub-input {
    flex: 1;
    font-size: 12px;
    padding: 2px 8px;
  }

  .template-panel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    margin-top: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
  }

  .history .task-row {
    opacity: 0.75;
  }
</style>
