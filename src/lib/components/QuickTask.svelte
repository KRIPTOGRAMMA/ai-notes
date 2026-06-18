<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit } from "@tauri-apps/api/event";
  import "../../app.css";

  let title = $state("");
  let priority = $state("Medium");

  async function create() {
    if (!title.trim()) return;
    await invoke("create_task", {
      task: {
        title,
        description: null,
        status: "Todo",
        priority,
        category: "Other",
        deadline: null,
        tags: [],
        recurrence: "None",
      }
    });
    await emit("task-created");
    await getCurrentWindow().hide();
    title = "";
  }

  async function cancel() {
    await getCurrentWindow().hide();
    title = "";
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") create();
    if (e.key === "Escape") cancel();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="container">
  <p class="label">Быстрая задача</p>
  <input
    bind:value={title}
    placeholder="Название..."
    autofocus
  />
  <select bind:value={priority}>
    <option value="Low">Низкий приоритет</option>
    <option value="Medium">Средний приоритет</option>
    <option value="High">Высокий приоритет</option>
    <option value="Critical">Критический</option>
  </select>
  <div class="buttons">
    <button onclick={create} disabled={!title.trim()}>Создать</button>
    <button onclick={cancel}>Отмена</button>
  </div>
</div>

<style>
  .container {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--bg-primary);
    height: 100vh;
  }
  .label {
    font-weight: 600;
    color: var(--text-primary);
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>