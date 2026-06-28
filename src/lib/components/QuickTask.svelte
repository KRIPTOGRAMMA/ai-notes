<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";
  import "../../app.css";

  let title = $state("");
  let description = $state("");
  let priority = $state("Medium");
  let category = $state("Work");
  let showDescription = $state(false);
  let errorMsg: string | null = $state(null);

  async function create() {
    if (!title.trim()) return;
    try {
      await api.createTask({
        title: title.trim(),
        description: description.trim() || null,
        status: "Todo",
        priority: priority as any,
        category: category as any,
        deadline: null,
        tags: [],
        recurrence: "None",
      });
      await emit("task-created");
      await getCurrentWindow().hide();
      title = "";
      description = "";
      priority = "Medium";
      category = "Work";
      showDescription = false;
      errorMsg = null;
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось создать задачу";
    }
  }

  async function cancel() {
    await getCurrentWindow().hide();
    title = "";
    description = "";
    showDescription = false;
    errorMsg = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) create();
    if (e.key === "Escape") cancel();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="container">
  <p class="label">Быстрая задача</p>

  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}

  <!-- svelte-ignore a11y_autofocus -->
  <input
    bind:value={title}
    placeholder="Название..."
    autofocus
  />

  <div class="row">
    <select bind:value={priority}>
      <option value="Low">Низкий</option>
      <option value="Medium">Средний</option>
      <option value="High">Высокий</option>
      <option value="Critical">Критический</option>
    </select>
    <select bind:value={category}>
      <option value="Work">Работа</option>
      <option value="Study">Учёба</option>
      <option value="Home">Дом</option>
      <option value="Health">Здоровье</option>
      <option value="Other">Другое</option>
    </select>
    <button class="desc-toggle" onclick={() => showDescription = !showDescription}>
      {showDescription ? "−" : "+ описание"}
    </button>
  </div>

  {#if showDescription}
    <textarea
      bind:value={description}
      placeholder="Описание..."
      rows="2"
    ></textarea>
  {/if}

  <div class="buttons">
    <button onclick={create} disabled={!title.trim()}>Создать</button>
    <button onclick={cancel}>Отмена</button>
  </div>
</div>

<style>
  .container {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-primary);
    height: 100vh;
    box-sizing: border-box;
  }
  .label {
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }
  .error {
    font-size: 12px;
    color: var(--danger);
    margin: 0;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row select {
    flex: 1;
  }
  .desc-toggle {
    white-space: nowrap;
    font-size: 12px;
    padding: 4px 8px;
  }
  textarea {
    resize: none;
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 2px;
  }
</style>
