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
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  onkeydown={() => {}}
  style="position:fixed;inset:0;background:rgba(0,0,0,0.4);display:flex;
    align-items:flex-start;justify-content:center;padding-top:12vh;z-index:1000;"
>
  <div style="width:min(560px,92vw);max-height:70vh;display:flex;flex-direction:column;
    background:var(--bg-card,#fff);border:1px solid var(--border,#e5e7eb);border-radius:10px;
    overflow:hidden;box-shadow:0 12px 40px rgba(0,0,0,0.3);">
    <input
      bind:this={inputEl}
      bind:value={query}
      oninput={runSearch}
      placeholder="Поиск задач и заметок..."
      style="border:none;border-bottom:1px solid var(--border,#e5e7eb);border-radius:0;
        padding:14px 16px;font-size:15px;background:transparent;outline:none;"
    />

    <div style="overflow-y:auto;">
      {#if !query.trim()}
        <p style="padding:16px;color:var(--text-secondary,#6b7280);font-size:13px;">
          Начните вводить запрос
        </p>
      {:else if taskResults.length === 0 && noteResults.length === 0}
        <p style="padding:16px;color:var(--text-secondary,#6b7280);font-size:13px;">
          Ничего не найдено
        </p>
      {:else}
        {#if taskResults.length > 0}
          <div style="padding:8px 16px 4px;font-size:11px;text-transform:uppercase;
            letter-spacing:.05em;color:var(--text-secondary,#6b7280);">Задачи</div>
          {#each taskResults as t (t.id)}
            <button
              onclick={() => onSelectTask(t.id)}
              style="display:block;width:100%;text-align:left;border:none;border-radius:0;
                background:transparent;padding:8px 16px;font:inherit;color:inherit;cursor:pointer;"
            >
              <div style="font-size:14px;">{t.title}</div>
              {#if t.description}
                <div style="font-size:12px;color:var(--text-secondary,#6b7280);
                  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">{t.description}</div>
              {/if}
            </button>
          {/each}
        {/if}

        {#if noteResults.length > 0}
          <div style="padding:8px 16px 4px;font-size:11px;text-transform:uppercase;
            letter-spacing:.05em;color:var(--text-secondary,#6b7280);">Заметки</div>
          {#each noteResults as n (n.id)}
            <button
              onclick={() => onSelectNote(n.id)}
              style="display:block;width:100%;text-align:left;border:none;border-radius:0;
                background:transparent;padding:8px 16px;font:inherit;color:inherit;cursor:pointer;"
            >
              <div style="font-size:14px;">{n.title}</div>
              <div style="font-size:12px;color:var(--text-secondary,#6b7280);
                white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">{n.content}</div>
            </button>
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</div>
