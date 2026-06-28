<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import type { Task } from "./lib/types";
  import "./app.css";

  let tasks: Task[] = $state([]);
  let editingId: string | null = $state(null);
  let editTitle = $state("");
  let showHistory = $state(false);
  let newTitle = $state("");
  let newRecurrence = $state("None");
  let newDeadline = $state("");
  let showForm = $state(false);
  let customN = $state(1);
  let customUnit = $state("Hours");
  let searchQuery = $state("");
  let searchResults: Task[] = $state([]);
  let isSearching = $state(false);
  let errorMsg: string | null = $state(null);
  let isDark = $state(
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );

  // Применяем тему сразу при инициализации компонента, а не только при клике
  // на кнопку. Раньше при тёмной системной теме переключатель визуально
  // ничего не делал на первый клик: isDark уже был true, но класс .dark
  // ни разу не вешался на <html>. Читаем matchMedia напрямую (не через
  // isDark) — это разовая инициализация DOM, а не реактивная подписка.
  if (typeof document !== "undefined") {
    document.documentElement.classList.toggle(
      "dark",
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  }

  function describeError(e: unknown): string {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message;
    return "Неизвестная ошибка";
  }

  async function loadTasks() {
    try {
      tasks = await invoke("get_tasks");
    } catch (e) {
      errorMsg = describeError(e);
    }
  }

  async function createTask() {
    try {
      const recurrence = buildRecurrence();
      await invoke("create_task", {
        task: {
          title: newTitle,
          description: null,
          status: "Todo",
          priority: "Medium",
          category: "Work",
          deadline: newRecurrence === "None" && newDeadline
            ? new Date(newDeadline).toISOString()
            : null,
          tags: [],
          recurrence,
        }
      });
      newTitle = "";
      newRecurrence = "None";
      newDeadline = "";
      showForm = false;
      await loadTasks();
    } catch (e) {
      errorMsg = describeError(e);
    }
  }

  function buildRecurrence() {
    switch (newRecurrence) {
      case "Hourly":  return "Hourly";
      case "Daily":   return "Daily";
      case "Weekly":  return "Weekly";
      case "Custom":  return { Custom: [customN, customUnit] };
      default:        return "None";
    }
  }

  async function deleteTask(id: string) {
    try {
      await invoke("delete_task", { id });
      await loadTasks();
    } catch (e) {
      errorMsg = describeError(e);
    }
  }

  async function completeTask(id: string) {
    try {
      await invoke("complete_task", { id });
      await loadTasks();
    } catch (e) {
      errorMsg = describeError(e);
    }
  }

  function startEdit(task: Task) {
    editingId = task.id;
    editTitle = task.title;
  }

  async function saveEdit() {
    try {
      await invoke("update_task", {
        id: editingId,
        patch: { title: editTitle },
      });
      editingId = null;
      await loadTasks();
    } catch (e) {
      errorMsg = describeError(e);
    }
  }

  async function search() {
    if (!searchQuery.trim()) {
      searchResults = [];
      return;
    }
    isSearching = true;
    try {
      searchResults = await invoke("search_tasks", { query: searchQuery });
    } catch (e) {
      errorMsg = describeError(e);
      searchResults = [];
    } finally {
      isSearching = false;
    }
  }

  function toggleTheme() {
    isDark = !isDark;
    document.documentElement.classList.toggle("dark", isDark);
  }

  // --- Активность: record_input описан в Rust как "вызывается с фронта
  // при mousemove/keydown", но раньше его никто не вызывал, поэтому
  // Active/Idle никогда не отражал реальную работу в приложении.
  // Троттлим, чтобы не дёргать invoke на каждый pixel движения мыши.
  let lastActivityPing = 0;
  function pingActivity() {
    const now = Date.now();
    if (now - lastActivityPing < 10_000) return;
    lastActivityPing = now;
    invoke("record_input").catch(() => {
      // тихо игнорируем — это фоновая телеметрия активности, не критично
    });
  }

  function priorityLabel(p: string) {
    switch (p) {
      case "Low": return "Низкий";
      case "Medium": return "Средний";
      case "High": return "Высокий";
      case "Critical": return "Критический";
      default: return p;
    }
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
    if (r === "Daily") return "Каждый день";
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

  let activeTasks = $derived(tasks.filter(t => !t.hidden));
  let historyTasks = $derived(tasks.filter(t => t.hidden));

  loadTasks();

  onMount(() => {
    const unlisten = listen("task-created", () => {
      loadTasks();
    });
    return () => {
      unlisten.then(fn => fn());
    };
  });
</script>

<!-- Раньше тут был один onkeydown только под quick-task хоткеи.
     Добавили mousemove/keydown пинг активности, не трогая существующую логику. -->
<svelte:window
  onmousemove={pingActivity}
  onkeydown={(e) => {
    pingActivity();
    if ((e.ctrlKey && e.code === 'KeyK') || (e.ctrlKey && e.shiftKey && e.code === 'KeyN')) {
      e.preventDefault();
      invoke("open_quick_task").catch((err) => { errorMsg = describeError(err); });
    }
  }}
/>

{#snippet taskMeta(task: Task)}
  {#if task.priority}
    {@const colors = priorityColors(task.priority)}
    <span style="font-size: 11px; padding: 2px 6px; border-radius: 4px; margin-left: 6px; font-weight: 500;
      background-color: {colors.bg}; color: {colors.fg};">
      {priorityLabel(task.priority)}
    </span>
  {/if}
  {#if task.deadline}
    <span style="color: var(--text-secondary); margin-left: 6px;">
      Дедлайн: {new Date(task.deadline).toLocaleString()}
    </span>
  {/if}
  {#if recurrenceLabel(task.recurrence)}
    <span style="color: var(--accent); margin-left: 6px; font-size: 12px; font-weight: 500;">
      [Повтор: {recurrenceLabel(task.recurrence)}]
    </span>
  {/if}
{/snippet}

{#if errorMsg}
  <div style="background: var(--danger); color: white; padding: 8px 12px; border-radius: 6px;
    margin-bottom: 10px; display: flex; justify-content: space-between; align-items: center; gap: 12px;">
    <span>{errorMsg}</span>
    <button onclick={() => errorMsg = null}
      style="background: transparent; border: none; color: white; padding: 2px 6px;">✕</button>
  </div>
{/if}

<button onclick={toggleTheme}>{isDark ? "Светлая" : "Тёмная"}</button>
<button onclick={loadTasks}>Обновить</button>
<button onclick={() => showHistory = !showHistory}>
  {showHistory ? "Скрыть историю" : "История"}
</button>
<button onclick={() => showForm = !showForm}>+ Новая задача</button>

<input
  bind:value={searchQuery}
  oninput={search}
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
          <button onclick={() => completeTask(task.id)}>Выполнить</button>
          <button onclick={() => deleteTask(task.id)}>Удалить</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<h2>Активные задачи</h2>
{#if activeTasks.length === 0}
  <p>Нет активных задач</p>
{:else}
  <ul>
    {#each activeTasks as task (task.id)}
      <li>
        {#if editingId === task.id}
          <input bind:value={editTitle} />
          <button onclick={saveEdit}>Сохранить</button>
          <button onclick={() => editingId = null}>Отмена</button>
        {:else}
          <strong>{task.title}</strong>
          {@render taskMeta(task)}
          <button onclick={() => startEdit(task)}>Изменить</button>
          <button onclick={() => completeTask(task.id)}>Выполнить</button>
          <button onclick={() => deleteTask(task.id)}>Удалить</button>
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
  {#if historyTasks.length === 0}
    <p>История пуста</p>
  {:else}
    <ul>
      {#each historyTasks as task (task.id)}
        <li>
          <strong>{task.title}</strong> — {task.status}
          <button onclick={() => deleteTask(task.id)}>Удалить</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
