<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit, listen } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";
  import { applyCachedTheme } from "../theme";
  import "../../app.css";

  type Mode = "task" | "note";
  let mode = $state<Mode>("task");

  // Задача
  let title = $state("");
  let description = $state("");
  let priority = $state("Medium");
  let category = $state("Work");
  let showDescription = $state(false);

  // Заметка
  let noteTitle = $state("");
  let noteContent = $state("");

  let errorMsg: string | null = $state(null);

  applyCachedTheme();

  onMount(() => {
    // Начальный режим — из managed-state (покрывает случай, когда окно уже было
    // смонтировано до эмита события).
    api.getQuickMode().then((m) => { mode = m; }).catch(() => {});
    // Живая смена режима, пока окно открыто.
    const un = listen<string>("quick-mode", (e) => {
      mode = e.payload === "note" ? "note" : "task";
    });
    return () => { un.then((f) => f()); };
  });

  function reset() {
    title = ""; description = ""; priority = "Medium"; category = "Work"; showDescription = false;
    noteTitle = ""; noteContent = "";
    errorMsg = null;
  }

  async function createTask() {
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
      reset();
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось создать задачу";
    }
  }

  async function createNote() {
    if (!noteTitle.trim() && !noteContent.trim()) return;
    try {
      await api.createNote({
        title: noteTitle.trim() || "Без названия",
        content: noteContent,
      });
      await emit("note-created");
      await getCurrentWindow().hide();
      reset();
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось создать заметку";
    }
  }

  function submit() {
    if (mode === "task") createTask(); else createNote();
  }

  async function cancel() {
    await getCurrentWindow().hide();
    reset();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      mode = mode === "task" ? "note" : "task";
      return;
    }
    if (e.key === "Escape") { cancel(); return; }
    // Enter создаёт: для задачи — в любом поле; для заметки — только с Ctrl
    // (обычный Enter в textarea переносит строку).
    if (e.key === "Enter" && !e.shiftKey) {
      if (mode === "task") { submit(); }
      else if (e.ctrlKey) { submit(); }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="container">
  <div class="tabs">
    <button class:active={mode === "task"} onclick={() => mode = "task"}>Задача</button>
    <button class:active={mode === "note"} onclick={() => mode = "note"}>Заметка</button>
    <span style="flex:1;"></span>
    <span class="hint">Ctrl+Tab</span>
  </div>

  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}

  {#if mode === "task"}
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={title} placeholder="Название задачи..." autofocus />

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
      <textarea bind:value={description} placeholder="Описание..." rows="2"></textarea>
    {/if}

    <div class="buttons">
      <button onclick={createTask} disabled={!title.trim()}>Создать</button>
      <button onclick={cancel}>Отмена</button>
    </div>
  {:else}
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={noteTitle} placeholder="Заголовок заметки..." autofocus />
    <textarea bind:value={noteContent} placeholder="Текст заметки... (Ctrl+Enter — сохранить)" rows="3"></textarea>

    <div class="buttons">
      <button onclick={createNote} disabled={!noteTitle.trim() && !noteContent.trim()}>Создать</button>
      <button onclick={cancel}>Отмена</button>
    </div>
  {/if}
</div>

<style>
  .container {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--bg-primary);
    height: 100vh;
    box-sizing: border-box;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tabs button {
    font-size: 13px;
    padding: 4px 10px;
  }
  .tabs button.active {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .hint {
    font-size: 11px;
    color: var(--text-secondary);
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
  .row select { flex: 1; }
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
