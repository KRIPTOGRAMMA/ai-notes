<script lang="ts">
  import { taskStore } from "../stores/tasks.svelte";
  import { noteStore } from "../stores/notes.svelte";
  import type { Task, Note } from "../types";

  let { onClose, onSelectTask, onSelectNote }: {
    onClose: () => void;
    onSelectTask: (id: string) => void;
    onSelectNote: (id: string) => void;
  } = $props();

  let query = $state("");
  let taskResults: Task[] = $state([]);
  let inputEl: HTMLInputElement | undefined = $state();

  // Заметки ищем клиентски (FTS для заметок нет), задачи — через готовый FTS.
  const noteResults = $derived.by<Note[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return noteStore.notes.filter(
      n => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q)
    );
  });

  let searchToken = 0;
  async function runSearch() {
    const q = query.trim();
    const token = ++searchToken;
    if (!q) { taskResults = []; return; }
    const res = await taskStore.search(q);
    if (token === searchToken) taskResults = res;
  }

  $effect(() => { inputEl?.focus(); });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div
  role="button"
  tabindex="-1"
  class="overlay backdrop"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  onkeydown={() => {}}
>
  <div class="modal panel">
    <input
      class="search-input"
      bind:this={inputEl}
      bind:value={query}
      oninput={runSearch}
      placeholder="Поиск задач и заметок..."
    />

    <div class="results">
      {#if !query.trim()}
        <p class="empty" style="padding:16px;">Начните вводить запрос</p>
      {:else if taskResults.length === 0 && noteResults.length === 0}
        <p class="empty" style="padding:16px;">Ничего не найдено</p>
      {:else}
        {#if taskResults.length > 0}
          <div class="group-title">Задачи</div>
          {#each taskResults as t (t.id)}
            <button class="result" onclick={() => onSelectTask(t.id)}>
              <div class="result-title">{t.title}</div>
              {#if t.description}
                <div class="result-sub">{t.description}</div>
              {/if}
            </button>
          {/each}
        {/if}

        {#if noteResults.length > 0}
          <div class="group-title">Заметки</div>
          {#each noteResults as n (n.id)}
            <button class="result" onclick={() => onSelectNote(n.id)}>
              <div class="result-title">{n.title}</div>
              <div class="result-sub">{n.content}</div>
            </button>
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 1000;
  }

  .panel {
    width: min(560px, 92vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search-input {
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    padding: 12px 16px;
    font-size: 14px;
    background: transparent;
  }
  .search-input:focus {
    outline: none;
    border-color: transparent;
    border-bottom-color: var(--accent);
  }

  .results {
    overflow-y: auto;
    padding: 4px;
  }

  .group-title {
    padding: 8px 12px 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .05em;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .result {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    padding: 6px 12px;
  }

  .result:hover {
    background: var(--bg-hover);
  }

  .result-title {
    font-size: 13px;
  }

  .result-sub {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
