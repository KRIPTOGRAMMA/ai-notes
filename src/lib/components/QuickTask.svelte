<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit } from "@tauri-apps/api/event";
  import "../../app.css";

  let title = $state("");
  let priority = $state("Medium");
  let errorMsg: string | null = $state(null);

  async function create() {
    if (!title.trim()) return;
    try {
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
      errorMsg = null;
    } catch (e) {
      // Раньше ошибка тут падала наружу: окно молча оставалось открытым,
      // title не очищался, и пользователь не понимал, что произошло.
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось создать задачу";
    }
  }

  async function cancel() {
    await getCurrentWindow().hide();
    title = "";
    errorMsg = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") create();
    if (e.key === "Escape") cancel();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="container">
  <p class="label">Быстрая задача</p>
  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}
  <!-- svelte-ignore a11y_autofocus -- это маленькое floating quick-capture
       окно (см. ТЗ 5.9), весь смысл которого в том, чтобы сразу печатать -->
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
  .error {
    font-size: 12px;
    color: var(--danger);
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>