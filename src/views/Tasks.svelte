<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { taskStore } from "../lib/stores/tasks";
  import type { Recurrence, RecurrenceUnit } from "../lib/types";

  let showHistory = $state(false);
  let showForm = $state(false);
  let editingId: string | null = $state(null);
  let editTitle = $state("");

  let newTitle = $state("");
  let newRecurrence = $state("None");
  let newDeadline = $state("");
  let customN = $state(1);
  let customUnit = $state("Hours");

  let searchQuery = $state("");
  let searchResults = $state<Awaited<ReturnType<typeof taskStore.search>>>([]);
  let isSearching = $state(false);

  function buildRecurrence(): Recurrence {
    switch (newRecurrence) {
      case "Hourly": return "Hourly";
      case "Daily":  return "Daily";
      case "Weekly": return "Weekly";
      case "Custom": return { Custom: [customN, customUnit as RecurrenceUnit] };
      default:       return "None";
    }
  }

  async function createTask() {
    await taskStore.create({
      title: newTitle,
      description: null,
      status: "Todo",
      priority: "Medium",
      category: "Work",
      deadline: newRecurrence === "None" && newDeadline
        ? new Date(newDeadline).toISOString()
        : null,
      tags: [],
      recurrence: buildRecurrence(),
    });
    newTitle = "";
    newRecurrence = "None";
    newDeadline = "";
    showForm = false;
  }

  function startEdit(id: string, title: string) {
    editingId = id;
    editTitle = title;
  }

  async function saveEdit() {
    if (!editingId) return;
    await taskStore.update(editingId, { title: editTitle });
    editingId = null;
  }

  async function handleSearch() {
    if (!searchQuery.trim()) { searchResults = []; return; }
    isSearching = true;
    searchResults = await taskStore.search(searchQuery);
    isSearching = false;
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
    return JSON.stringify(r);
  }

  taskStore.load();

  onMount(() => {
    const unlisten = listen("task-created", () => taskStore.load());
    return () => { unlisten.then(fn => fn()); };
  });
</script>

{#snippet taskMeta(task: import("../lib/types").Task)}
  {#if task.priority}
    {@const colors = priorityColors(task.priority)}
    <span style="font-size:11px;padding:2px 6px;border-radius:4px;margin-left:6px;font-weight:500;
      background-color:{colors.bg};color:{colors.fg};">
      {priorityLabel(task.priority)}
    </span>
  {/if}
  {#if task.deadline}
    <span style="color:var(--text-secondary);margin-left:6px;">
      Дедлайн: {new Date(task.deadline).toLocaleString()}
    </span>
  {/if}
  {#if recurrenceLabel(task.recurrence)}
    <span style="color:var(--accent);margin-left:6px;font-size:12px;font-weight:500;">
      [Повтор: {recurrenceLabel(task.recurrence)}]
    </span>
  {/if}
{/snippet}

<div>
  <button onclick={() => taskStore.load()}>Обновить</button>
  <button onclick={() => showHistory = !showHistory}>
    {showHistory ? "Скрыть историю" : "История"}
  </button>
  <button onclick={() => showForm = !showForm}>+ Новая задача</button>

  <input
    bind:value={searchQuery}
    oninput={handleSearch}
    placeholder="Поиск задач..."
  />

  {#if searchQuery.trim()}
    <h2>Результаты поиска</h2>
    {#if isSearching}
      <p>Поиск...</p>
    {:else if searchResults.length === 0}
      <p>Ничего не найдено</p>
    {:else}
      <ul>
        {#each searchResults as task (task.id)}
          <li>
            <strong>{task.title}</strong>
            {@render taskMeta(task)}
            <button onclick={() => taskStore.complete(task.id)}>Выполнить</button>
            <button onclick={() => taskStore.remove(task.id)}>Удалить</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  <h2>Активные задачи</h2>
  {#if taskStore.activeTasks.length === 0}
    <p>Нет активных задач</p>
  {:else}
    <ul>
      {#each taskStore.activeTasks as task (task.id)}
        <li>
          {#if editingId === task.id}
            <input bind:value={editTitle} />
            <button onclick={saveEdit}>Сохранить</button>
            <button onclick={() => editingId = null}>Отмена</button>
          {:else}
            <strong>{task.title}</strong>
            {@render taskMeta(task)}
            <button onclick={() => startEdit(task.id, task.title)}>Изменить</button>
            <button onclick={() => taskStore.complete(task.id)}>Выполнить</button>
            <button onclick={() => taskStore.remove(task.id)}>Удалить</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if showForm}
    <div>
      <input bind:value={newTitle} placeholder="Название задачи" />
      <select bind:value={newRecurrence}>
        <option value="None">Без повтора</option>
        <option value="Hourly">Каждый час</option>
        <option value="Daily">Каждый день</option>
        <option value="Weekly">Каждую неделю</option>
        <option value="Custom">Свой интервал</option>
      </select>
      {#if newRecurrence === "Custom"}
        <input type="number" bind:value={customN} min="1" />
        <select bind:value={customUnit}>
          <option value="Hours">Часов</option>
          <option value="Days">Дней</option>
          <option value="Weeks">Недель</option>
        </select>
      {/if}
      {#if newRecurrence === "None"}
        <input type="datetime-local" bind:value={newDeadline} />
      {/if}
      <button onclick={createTask} disabled={!newTitle.trim()}>Создать</button>
    </div>
  {/if}

  {#if showHistory}
    <h2>История</h2>
    {#if taskStore.historyTasks.length === 0}
      <p>История пуста</p>
    {:else}
      <ul>
        {#each taskStore.historyTasks as task (task.id)}
          <li>
            <strong>{task.title}</strong> — {task.status}
            <button onclick={() => taskStore.remove(task.id)}>Удалить</button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>
