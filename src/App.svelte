<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import "./app.css";

  let tasks = $state([]);
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
  let searchResults = $state([]);
  let isSearching = $state(false);
  let isDark = $state(
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );

  async function loadTasks() {
    tasks = await invoke("get_tasks");
  }

  async function createTask() {
    let recurrence = buildRecurrence();
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
    await invoke("delete_task", { id });
    await loadTasks();
  }

  async function completeTask(id: string) {
    await invoke("complete_task", { id });
    await loadTasks();
  }

  function startEdit(task) {
    editingId = task.id;
    editTitle = task.title;
  }

  async function saveEdit() {
    await invoke("update_task", {
      id: editingId,
      patch: { title: editTitle },
    });
    editingId = null;
    await loadTasks();
  }

  async function search() {
    if (!searchQuery.trim()) {
      searchResults = [];
      return;
    }
    isSearching = true;
    searchResults = await invoke("search_tasks", { query: searchQuery });
    isSearching = false;
  }

  function toggleTheme() {
    isDark = !isDark;
    document.documentElement.classList.toggle("dark", isDark);
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

<svelte:window onkeydown={(e) => {
  if ((e.ctrlKey && e.code === 'KeyK') || (e.ctrlKey && e.shiftKey && e.code === 'KeyN')) {
    e.preventDefault();
    invoke("open_quick_task");
  }
}} />

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
      {#each searchResults as task}
        <li>
          <strong>{task.title}</strong>
          {#if task.priority}
            <span style="font-size: 11px; padding: 2px 6px; border-radius: 4px; margin-left: 6px; font-weight: 500;
              background-color: {
                task.priority === 'Low' ? '#e5e7eb' : 
                task.priority === 'Medium' ? '#dbeafe' : 
                task.priority === 'High' ? '#ffedd5' : 
                task.priority === 'Critical' ? '#fee2e2' : '#f3f4f6'
              };
              color: {
                task.priority === 'Low' ? '#4b5563' : 
                task.priority === 'Medium' ? '#2563eb' : 
                task.priority === 'High' ? '#ea580c' : 
                task.priority === 'Critical' ? '#dc2626' : '#1f2937'
              };">
              {task.priority === 'Low' ? 'Низкий' : 
               task.priority === 'Medium' ? 'Средний' : 
               task.priority === 'High' ? 'Высокий' : 
               task.priority === 'Critical' ? 'Критический' : task.priority}
            </span>
          {/if}
          {#if task.deadline}
            <span style="color: var(--text-secondary); margin-left: 6px;">
              Дедлайн: {new Date(task.deadline).toLocaleString()}
            </span>
          {/if}
          {#if task.recurrence && task.recurrence !== 'None'}
            <span style="color: var(--accent); margin-left: 6px; font-size: 12px; font-weight: 500;">
              [Повтор: {task.recurrence === 'Hourly' ? 'Каждый час' : 
                        task.recurrence === 'Daily' ? 'Каждый день' : 
                        task.recurrence === 'Weekly' ? 'Каждую неделю' : 
                        typeof task.recurrence === 'object' && 'Custom' in task.recurrence ? `раз в ${task.recurrence.Custom[0]} ${task.recurrence.Custom[1] === 'Minutes' ? 'мин.' : task.recurrence.Custom[1] === 'Hours' ? 'ч.' : task.recurrence.Custom[1] === 'Days' ? 'дн.' : 'нед.'}` : 
                        JSON.stringify(task.recurrence)}]
            </span>
          {/if}
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
    {#each activeTasks as task}
      <li>
        {#if editingId === task.id}
          <input bind:value={editTitle} />
          <button onclick={saveEdit}>Сохранить</button>
          <button onclick={() => editingId = null}>Отмена</button>
        {:else}
          <strong>{task.title}</strong>
          {#if task.priority}
            <span style="font-size: 11px; padding: 2px 6px; border-radius: 4px; margin-left: 6px; font-weight: 500;
              background-color: {
                task.priority === 'Low' ? '#e5e7eb' : 
                task.priority === 'Medium' ? '#dbeafe' : 
                task.priority === 'High' ? '#ffedd5' : 
                task.priority === 'Critical' ? '#fee2e2' : '#f3f4f6'
              };
              color: {
                task.priority === 'Low' ? '#4b5563' : 
                task.priority === 'Medium' ? '#2563eb' : 
                task.priority === 'High' ? '#ea580c' : 
                task.priority === 'Critical' ? '#dc2626' : '#1f2937'
              };">
              {task.priority === 'Low' ? 'Низкий' : 
               task.priority === 'Medium' ? 'Средний' : 
               task.priority === 'High' ? 'Высокий' : 
               task.priority === 'Critical' ? 'Критический' : task.priority}
            </span>
          {/if}
          {#if task.deadline}
            <span style="color: var(--text-secondary); margin-left: 6px;">
              Дедлайн: {new Date(task.deadline).toLocaleString()}
            </span>
          {/if}
          {#if task.recurrence && task.recurrence !== 'None'}
            <span style="color: var(--accent); margin-left: 6px; font-size: 12px; font-weight: 500;">
              [Повтор: {task.recurrence === 'Hourly' ? 'Каждый час' : 
                        task.recurrence === 'Daily' ? 'Каждый день' : 
                        task.recurrence === 'Weekly' ? 'Каждую неделю' : 
                        typeof task.recurrence === 'object' && 'Custom' in task.recurrence ? `раз в ${task.recurrence.Custom[0]} ${task.recurrence.Custom[1] === 'Minutes' ? 'мин.' : task.recurrence.Custom[1] === 'Hours' ? 'ч.' : task.recurrence.Custom[1] === 'Days' ? 'дн.' : 'нед.'}` : 
                        JSON.stringify(task.recurrence)}]
            </span>
          {/if}
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
      {#each historyTasks as task}
        <li>
          <strong>{task.title}</strong> — {task.status}
          <button onclick={() => deleteTask(task.id)}>Удалить</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}