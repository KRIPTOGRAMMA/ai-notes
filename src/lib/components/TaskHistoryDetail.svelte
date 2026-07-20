<script lang="ts">
  import type { Task } from "../types";
  import { categoryStore } from "../stores/categories.svelte";
  import { projectStore } from "../stores/projects.svelte";

  type Props = {
    task: Task;
    onClose: () => void;
  };

  let { task, onClose }: Props = $props();

  const projectName = $derived(
    task.project_id ? (projectStore.projects.find(p => p.id === task.project_id)?.name ?? null) : null
  );

  function formatDate(iso: string | null): string {
    if (!iso) return "—";
    return new Date(iso).toLocaleString([], { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div role="dialog" aria-modal="true" class="overlay backdrop" onclick={handleBackdropClick}>
  <div class="modal dialog">
    <h2 class="dialog-title">{task.title}</h2>

    {#if task.description}
      <div class="field">
        <span class="label">Описание</span>
        <div class="desc-text">{task.description}</div>
      </div>
    {/if}

    {#if task.subtasks.length > 0}
      <div class="field">
        <span class="label">Подзадачи</span>
        <div class="checklist">
          {#each task.subtasks as sub (sub.id)}
            <div class="check-row">
              <input type="checkbox" checked={sub.done} disabled />
              <span class:sub-done={sub.done}>{sub.title}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <div class="pair">
      <div class="field">
        <span class="label">Категория</span>
        <span class="chip chip-cat" style="--cat: {categoryStore.color(task.category)}">{categoryStore.name(task.category)}</span>
      </div>
      {#if projectName}
        <div class="field">
          <span class="label">Проект</span>
          <span class="muted">{projectName}</span>
        </div>
      {/if}
    </div>

    {#if task.tags.length > 0}
      <div class="field">
        <span class="label">Теги</span>
        <div class="tag-row">
          {#each task.tags as tag}
            <span class="chip chip-tag">#{tag}</span>
          {/each}
        </div>
      </div>
    {/if}

    <div class="pair">
      <div class="field">
        <span class="label">Создана</span>
        <span class="muted" style="font-size:13px;">{formatDate(task.created_at)}</span>
      </div>
      <div class="field">
        <span class="label">Завершена</span>
        <span class="muted" style="font-size:13px;">{formatDate(task.completed_at)}</span>
      </div>
    </div>

    <div class="actions">
      <button class="btn-primary" onclick={onClose}>Закрыть</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    align-items: center;
    padding: 16px;
  }

  .dialog {
    width: 100%;
    max-width: 500px;
    max-height: 90vh;
    overflow-y: auto;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dialog-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .desc-text {
    font-size: 13px;
    white-space: pre-wrap;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .checklist {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .sub-done {
    text-decoration: line-through;
    color: var(--text-secondary);
  }

  .tag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }
</style>
