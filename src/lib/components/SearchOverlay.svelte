<script lang="ts">
  import { taskStore } from "../stores/tasks.svelte";
  import { api } from "../api/tauri";
  import type { TaskSnippet, NoteSnippet } from "../types";

  interface PaletteCommand {
    label: string;
    hint?: string;
    keywords: string;
    run: () => void;
  }

  let { commands = [] as PaletteCommand[], onClose, onSelectTask, onSelectNote }: {
    commands?: PaletteCommand[];
    onClose: () => void;
    onSelectTask: (id: string) => void;
    onSelectNote: (id: string) => void;
  } = $props();

  let query = $state("");
  let taskResults: TaskSnippet[] = $state([]);
  let noteResults: NoteSnippet[] = $state([]);
  let inputEl: HTMLInputElement | undefined = $state();
  let activeIndex = $state(0);
  let items: Array<{ type: "command"; i: number } | { type: "task"; i: number } | { type: "note"; i: number }> = $state([]);
  // Команды, прошедшие фильтр текущего запроса — items с type "command" индексируют
  // СЮДА, а не в исходный commands[] (иначе после фильтрации индексы съезжают
  // и клик/рендер попадают на другую команду — был баг).
  let filteredCommands: PaletteCommand[] = $state([]);

  let searchToken = 0;
  async function runSearch() {
    const q = query.trim();
    const token = ++searchToken;
    let tasks: TaskSnippet[] = [];
    let notes: NoteSnippet[] = [];
    if (q) {
      const res = await Promise.all([
        api.searchTasksSnippet(q),
        api.searchNotesSnippet(q),
      ]);
      tasks = res[0];
      notes = res[1];
    }
    if (token !== searchToken) return;
    taskResults = tasks;
    noteResults = notes;
    rebuild(q, tasks, notes);
  }

  function rebuild(q: string, tasks: TaskSnippet[], notes: NoteSnippet[]) {
    const lq = q.toLowerCase();
    filteredCommands = commands.filter(c => !lq || c.keywords.includes(lq) || c.label.toLowerCase().includes(lq));
    items = [
      ...filteredCommands.map((_, i) => ({ type: "command" as const, i })),
      ...tasks.map((_, i) => ({ type: "task" as const, i })),
      ...notes.map((_, i) => ({ type: "note" as const, i })),
    ];
    if (activeIndex >= items.length) activeIndex = items.length > 0 ? 0 : -1;
  }

  $effect(() => { inputEl?.focus(); });
  // Стартовая сборка
  $effect(() => {
    if (items.length > 0) return;
    rebuild("", [], []);
  });

  function execute(idx: number) {
    const item = items[idx];
    if (!item) return;
    if (item.type === "command") { filteredCommands[item.i].run(); onClose(); }
    else if (item.type === "task") { onSelectTask(taskResults[item.i].item.id); }
    else if (item.type === "note") { onSelectNote(noteResults[item.i].item.id); }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); activeIndex = Math.min(activeIndex + 1, items.length - 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); activeIndex = Math.max(activeIndex - 1, 0); }
    else if (e.key === "Enter" && activeIndex >= 0) { e.preventDefault(); execute(activeIndex); }
  }

  function renderHtml(s: string) {
    return s.replace(/</g, "&lt;").replace(/<mark>/g, "<mark>").replace(/<\/mark>/g, "</mark>");
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
      {#if items.length === 0}
        <p class="empty" style="padding:16px;">
          {query.trim() ? "Ничего не найдено" : "Начните вводить запрос"}
        </p>
      {:else}
        {#each items as item, i (item)}
          {#if i > 0 && item.type !== items[i - 1].type}
            <div class="group-title-sep"></div>
          {/if}
          {#if item.type === "command"}
            {@const cmd = filteredCommands[item.i]}
            <button
              class="result" class:active={i === activeIndex}
              onclick={() => execute(i)}
              onmouseenter={() => activeIndex = i}
            >
              <div class="result-title">{cmd.label}</div>
              {#if cmd.hint}
                <div class="result-sub">{cmd.hint}</div>
              {/if}
            </button>
          {:else if item.type === "task"}
            {@const r = taskResults[item.i]}
            <button
              class="result" class:active={i === activeIndex}
              onclick={() => execute(i)}
              onmouseenter={() => activeIndex = i}
            >
              <div class="result-title">{r.item.title}</div>
              {#if r.snippet}
                <div class="result-sub">{@html renderHtml(r.snippet)}</div>
              {/if}
            </button>
          {:else}
            {@const r = noteResults[item.i]}
            <button
              class="result" class:active={i === activeIndex}
              onclick={() => execute(i)}
              onmouseenter={() => activeIndex = i}
            >
              <div class="result-title">{r.item.title}</div>
              {#if r.snippet}
                <div class="result-sub">{@html renderHtml(r.snippet)}</div>
              {:else}
                <div class="result-sub">{r.item.content}</div>
              {/if}
            </button>
          {/if}
        {/each}
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

  .group-title-sep {
    height: 1px;
    margin: 4px 12px;
    background: var(--border);
  }

  .result {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    padding: 6px 12px;
    cursor: pointer;
  }

  .result:hover,
  .result.active {
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

  .result-sub :global(mark) {
    background: var(--accent);
    color: var(--bg-primary);
    border-radius: 2px;
    padding: 0 2px;
  }
</style>
