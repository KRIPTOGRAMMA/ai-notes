<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { api } from "../lib/api/tauri";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import type { Task, Category, CreateTaskPayload, UpdateTaskPayload } from "../lib/types";

  type AiResult = { task_id: string; type: string; result?: string; error?: string };

  let showHistory = $state(false);
  let showCreateModal = $state(false);
  let editingTask: Task | null = $state(null);

  let searchQuery = $state("");
  let searchResults = $state<Task[]>([]);
  let isSearching = $state(false);

  let aiLoadingId: string | null = $state(null);
  let aiError: string | null = $state(null);
  let subtasksPreview: { taskId: string; items: string[] } | null = $state(null);

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

  let collapsed = $state<Record<string, boolean>>({});
  const doneCount = (t: Task) => t.subtasks.filter((s) => s.done).length;

  async function classifyTask(id: string, title: string) {
    aiLoadingId = id;
    aiError = null;
    await api.aiClassify(id, title);
  }

  function priorityLabel(p: string) {
    const map: Record<string, string> = {
      Low: "Низкий", Medium: "Средний", High: "Высокий", Critical: "Критический",
    };
    return map[p] ?? p;
  }

  const PRIORITY_COLORS: Record<string, { bg: string; fg: string }> = {
    Low:      { bg: "#e5e7eb", fg: "#4b5563" },
    Medium:   { bg: "#dbeafe", fg: "#2563eb" },
    High:     { bg: "#ffedd5", fg: "#ea580c" },
    Critical: { bg: "#fee2e2", fg: "#dc2626" },
  };

  function priorityColors(p: string) {
    return PRIORITY_COLORS[p] ?? { bg: "#f3f4f6", fg: "#1f2937" };
  }

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

  taskStore.load();

  onMount(() => {
    const unlistenTask = listen("task-created", () => taskStore.load());

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

    return () => {
      unlistenTask.then(fn => fn());
      unlistenAi.then(fn => fn());
    };
  });
</script>

{#snippet taskMeta(task: Task)}
  {@const colors = priorityColors(task.priority)}
  <span style="font-size:11px;padding:2px 6px;border-radius:4px;margin-left:6px;font-weight:500;
    background-color:{colors.bg};color:{colors.fg};">
    {priorityLabel(task.priority)}
  </span>
  <span style="font-size:11px;padding:2px 6px;border-radius:4px;margin-left:6px;background:#f3f4f6;color:#6b7280;">
    {task.category}
  </span>
  {#if task.tags.length > 0}
    {#each task.tags as tag}
      <span style="font-size:11px;padding:2px 6px;border-radius:4px;margin-left:4px;background:#e0f2fe;color:#0369a1;">#{tag}</span>
    {/each}
  {/if}
  {#if task.deadline}
    <span style="color:var(--text-secondary,#6b7280);margin-left:6px;font-size:12px;">
      Дедлайн: {new Date(task.deadline).toLocaleString()}
    </span>
  {/if}
  {#if recurrenceLabel(task.recurrence)}
    <span style="color:#7c3aed;margin-left:6px;font-size:12px;font-weight:500;">
      ↻ {recurrenceLabel(task.recurrence)}
    </span>
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

<div>
  {#if aiError}
    <div style="background:#ef4444;color:white;padding:6px 10px;border-radius:6px;margin-bottom:8px;display:flex;justify-content:space-between;">
      <span>{aiError}</span>
      <button onclick={() => aiError = null} style="background:transparent;border:none;color:white;cursor:pointer;">✕</button>
    </div>
  {/if}

  <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:12px;">
    <button onclick={() => showCreateModal = true}>+ Новая задача</button>
    <button onclick={() => taskStore.load()}>Обновить</button>
    <button onclick={() => showHistory = !showHistory}>
      {showHistory ? "Скрыть историю" : "История"}
    </button>
    <input
      bind:value={searchQuery}
      oninput={handleSearch}
      placeholder="Поиск задач..."
      style="flex:1;min-width:150px;"
    />
  </div>

  {#if searchQuery.trim()}
    <h2>Результаты поиска</h2>
    {#if isSearching}
      <p>Поиск...</p>
    {:else if searchResults.length === 0}
      <p>Ничего не найдено</p>
    {:else}
      <ul>
        {#each searchResults as task (task.id)}
          <li style="margin-bottom:10px;">
            <strong>{task.title}</strong>
            {@render taskMeta(task)}
            {#if task.description}
              <p style="margin:4px 0 0 0;font-size:13px;color:var(--text-secondary,#6b7280);">{task.description}</p>
            {/if}
            <div style="margin-top:4px;display:flex;gap:4px;">
              <button onclick={() => taskStore.complete(task.id)}>Выполнить</button>
              <button onclick={() => taskStore.remove(task.id)}>Удалить</button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  <h2>Активные задачи</h2>
  {#if taskStore.activeTasks.length === 0}
    <p>Нет активных задач</p>
  {:else}
    <ul style="list-style:none;padding:0;margin:0;">
      {#each taskStore.activeTasks as task (task.id)}
        <li style="margin-bottom:14px;padding:10px;border:1px solid var(--border,#e5e7eb);border-radius:8px;">
          <div style="display:flex;align-items:flex-start;gap:8px;flex-wrap:wrap;">
            <strong style="font-size:15px;">{task.title}</strong>
            {@render taskMeta(task)}
          </div>

          {#if task.description}
            <p style="margin:6px 0 0 0;font-size:13px;color:var(--text-secondary,#6b7280);">{task.description}</p>
          {/if}

          <div style="margin-top:8px;display:flex;gap:4px;flex-wrap:wrap;">
            <button onclick={() => editingTask = task}>Изменить</button>
            <button
              onclick={() => rewriteTask(task.id, task.title)}
              disabled={aiLoadingId === task.id}
              title="Переформулировать в SMART"
            >{aiLoadingId === task.id ? "..." : "✨ SMART"}</button>
            <button
              onclick={() => generateSubtasks(task.id, task.title)}
              disabled={aiLoadingId === task.id}
              title="Разбить на подзадачи"
            >{aiLoadingId === task.id ? "..." : "🔀 Подзадачи"}</button>
            <button
              onclick={() => classifyTask(task.id, task.title)}
              disabled={aiLoadingId === task.id}
              title="Авто-категория"
            >{aiLoadingId === task.id ? "..." : "🏷 Категория"}</button>
            <button onclick={() => taskStore.complete(task.id)}>Выполнить</button>
            <button onclick={() => taskStore.remove(task.id)}>Удалить</button>
          </div>

          {#if subtasksPreview && subtasksPreview.taskId === task.id}
            <div style="margin-top:8px;padding:10px;background:var(--bg-secondary,#f9fafb);border-radius:6px;">
              <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
                <span style="font-size:12px;font-weight:600;">ИИ предлагает подзадачи:</span>
                <button onclick={() => acceptAllSubtasks(task.id, subtasksPreview!.items)}>Принять все</button>
              </div>
              {#each subtasksPreview.items as subtask}
                <div style="display:flex;align-items:center;gap:8px;margin-bottom:4px;">
                  <span style="font-size:13px;flex:1;">{subtask}</span>
                  <button onclick={() => acceptSubtask(task.id, subtask)}>+ Добавить</button>
                </div>
              {/each}
              <button onclick={() => subtasksPreview = null} style="margin-top:4px;">Закрыть</button>
            </div>
          {/if}

          <!-- Чек-лист подзадач: вложен в задачу, прогресс N/M -->
          {#if task.subtasks.length > 0}
            <div style="margin-top:8px;">
              <button
                onclick={() => collapsed[task.id] = !collapsed[task.id]}
                style="display:flex;align-items:center;gap:8px;background:none;border:none;padding:0;cursor:pointer;font-size:13px;font-weight:600;color:var(--text,#374151);"
              >
                <span>{collapsed[task.id] ? "▸" : "▾"}</span>
                Подзадачи
                <span style="color:var(--text-secondary,#6b7280);font-weight:400;">
                  {doneCount(task)}/{task.subtasks.length}
                </span>
              </button>
              {#if !collapsed[task.id]}
                <div style="margin:6px 0 0 18px;display:flex;flex-direction:column;gap:4px;">
                  {#each task.subtasks as sub (sub.id)}
                    <div style="display:flex;align-items:center;gap:8px;">
                      <input type="checkbox" checked={sub.done} onchange={() => toggleSubtask(sub.id)} />
                      <span style="font-size:13px;flex:1;{sub.done ? 'text-decoration:line-through;color:var(--text-secondary,#9ca3af);' : ''}">{sub.title}</span>
                      <button onclick={() => removeSubtask(sub.id)} title="Удалить" style="font-size:12px;">✕</button>
                    </div>
                  {/each}
                  <div style="display:flex;gap:6px;margin-top:2px;">
                    <input
                      type="text"
                      placeholder="+ подзадача"
                      bind:value={newSubtaskInput[task.id]}
                      onkeydown={(e) => { if (e.key === 'Enter') addManualSubtask(task.id); }}
                      style="flex:1;font-size:12px;padding:2px 6px;"
                    />
                    <button onclick={() => addManualSubtask(task.id)}>Добавить</button>
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if showHistory}
    <h2>История</h2>
    {#if taskStore.historyTasks.length === 0}
      <p>История пуста</p>
    {:else}
      <ul style="list-style:none;padding:0;margin:0;">
        {#each taskStore.historyTasks as task (task.id)}
          <li style="margin-bottom:8px;padding:8px;border:1px solid var(--border,#e5e7eb);border-radius:6px;opacity:0.7;">
            <strong>{task.title}</strong>
            <span style="margin-left:6px;font-size:12px;color:var(--text-secondary,#6b7280);">{task.status}</span>
            {#if task.description}
              <p style="margin:2px 0 0 0;font-size:13px;color:var(--text-secondary,#6b7280);">{task.description}</p>
            {/if}
            <button onclick={() => taskStore.remove(task.id)} style="margin-top:4px;">Удалить</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>
