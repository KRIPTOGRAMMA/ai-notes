<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
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
</script>

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
          <strong>{task.title}</strong> — {task.status}
          {#if task.deadline}
            <span>{new Date(task.deadline).toLocaleString()}</span>
          {/if}
          <button onclick={() => completeTask(task.id)}>✓ Выполнить</button>
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
          {#if task.deadline}
            <span>{new Date(task.deadline).toLocaleString()}</span>
          {/if}
          <span>{JSON.stringify(task.recurrence)}</span>
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