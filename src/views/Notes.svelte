<script lang="ts">
  import { onMount, tick } from "svelte";
  import { noteStore } from "../lib/stores/notes.svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { renderMarkdown, toggleTaskListItem } from "../lib/markdown";
  import type { Note } from "../lib/types";

  let selectedId: string | null = $state(null);
  let editTitle = $state("");
  let editContent = $state("");
  let editTags: string[] = $state([]);
  let editLinkedTaskId: string | null = $state(null);
  let tagInput = $state("");
  let previewMode = $state(false);
  let previewEl: HTMLDivElement | undefined = $state();
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;
  let saving = $state(false);

  const selected = $derived(noteStore.notes.find(n => n.id === selectedId) ?? null);
  const previewHtml = $derived(renderMarkdown(editContent));

  function selectNote(note: Note) {
    selectedId = note.id;
    editTitle = note.title;
    editContent = note.content;
    editTags = [...note.tags];
    editLinkedTaskId = note.linked_task_id;
    previewMode = false;
  }

  // Открытие заметки по сигналу из глобального поиска (Ctrl+K).
  $effect(() => {
    const id = noteStore.focusNoteId;
    if (!id) return;
    const note = noteStore.notes.find(n => n.id === id);
    if (note) selectNote(note);
    noteStore.clearFocus();
  });

  async function newNote() {
    const note = await noteStore.create({ title: "Без названия", content: "" });
    if (note) selectNote(note);
  }

  function scheduleSave() {
    if (!selectedId) return;
    if (saveTimeout) clearTimeout(saveTimeout);
    saving = true;
    saveTimeout = setTimeout(async () => {
      await noteStore.update(selectedId!, { title: editTitle, content: editContent });
      saving = false;
    }, 800);
  }

  // Теги и привязка сохраняются сразу (без дебаунса).
  async function saveMeta() {
    if (!selectedId) return;
    await noteStore.update(selectedId, { tags: editTags, linked_task_id: editLinkedTaskId });
  }

  function addTag() {
    const t = tagInput.trim();
    if (t && !editTags.includes(t)) {
      editTags = [...editTags, t];
      saveMeta();
    }
    tagInput = "";
  }

  function removeTag(tag: string) {
    editTags = editTags.filter(t => t !== tag);
    saveMeta();
  }

  function onTagKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); addTag(); }
  }

  async function deleteSelected() {
    if (!selectedId) return;
    await noteStore.remove(selectedId);
    selectedId = null;
    editTitle = "";
    editContent = "";
    editTags = [];
    editLinkedTaskId = null;
  }

  // Интерактивные чек-листы: после рендера превью снимаем disabled с чекбоксов
  // и по клику переключаем соответствующую строку в самом markdown-тексте.
  $effect(() => {
    // зависимости: перечитываем при смене html/режима
    previewHtml; previewMode;
    if (!previewMode || !previewEl) return;
    tick().then(() => {
      if (!previewEl) return;
      const boxes = previewEl.querySelectorAll<HTMLInputElement>('input[type="checkbox"]');
      boxes.forEach((box, idx) => {
        box.disabled = false;
        box.onchange = () => {
          editContent = toggleTaskListItem(editContent, idx);
          scheduleSave();
        };
      });
    });
  });

  function formatDate(iso: string) {
    return new Date(iso).toLocaleDateString("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  }

  const linkedTask = $derived(
    editLinkedTaskId ? taskStore.tasks.find(t => t.id === editLinkedTaskId) ?? null : null
  );

  onMount(() => {
    noteStore.load();
    taskStore.load();
  });
</script>

<div style="display:flex;height:100%;gap:0;">
  <!-- Sidebar -->
  <div style="width:220px;min-width:180px;border-right:1px solid var(--border,#e5e7eb);display:flex;flex-direction:column;">
    <div style="padding:10px;border-bottom:1px solid var(--border,#e5e7eb);">
      <button onclick={newNote} style="width:100%;">+ Новая заметка</button>
    </div>

    {#if noteStore.notes.length === 0}
      <p style="padding:12px;color:var(--text-secondary,#6b7280);font-size:13px;">Нет заметок</p>
    {:else}
      <ul style="list-style:none;padding:0;margin:0;overflow-y:auto;flex:1;">
        {#each noteStore.notes as note (note.id)}
          <li style="border-bottom:1px solid var(--border,#e5e7eb);">
            <button
              onclick={() => selectNote(note)}
              style="display:block;width:100%;text-align:left;padding:10px 12px;cursor:pointer;
                border:none;border-radius:0;font:inherit;color:inherit;
                background:{selectedId === note.id ? 'var(--accent-light,#eff6ff)' : 'transparent'};"
            >
              <div style="font-size:13px;font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
                {note.title}
              </div>
              <div style="font-size:11px;color:var(--text-secondary,#6b7280);margin-top:2px;">
                {formatDate(note.updated_at)}
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <!-- Editor -->
  <div style="flex:1;display:flex;flex-direction:column;overflow:hidden;">
    {#if !selected}
      <div style="flex:1;display:flex;align-items:center;justify-content:center;color:var(--text-secondary,#6b7280);">
        Выберите заметку или создайте новую
      </div>
    {:else}
      <div style="padding:10px 14px;border-bottom:1px solid var(--border,#e5e7eb);display:flex;align-items:center;gap:8px;">
        <input
          bind:value={editTitle}
          oninput={scheduleSave}
          placeholder="Название"
          style="flex:1;font-size:15px;font-weight:600;border:none;outline:none;background:transparent;"
        />
        {#if saving}
          <span style="font-size:11px;color:var(--text-secondary,#6b7280);">Сохранение...</span>
        {/if}
        <div style="display:flex;border:1px solid var(--border,#e5e7eb);border-radius:6px;overflow:hidden;">
          <button onclick={() => previewMode = false}
            style="border:none;border-radius:0;font-size:12px;padding:4px 10px;
              background:{previewMode ? 'transparent' : 'var(--accent,#6366f1)'};
              color:{previewMode ? 'inherit' : '#fff'};">Редактировать</button>
          <button onclick={() => previewMode = true}
            style="border:none;border-radius:0;font-size:12px;padding:4px 10px;
              background:{previewMode ? 'var(--accent,#6366f1)' : 'transparent'};
              color:{previewMode ? '#fff' : 'inherit'};">Превью</button>
        </div>
        <button onclick={deleteSelected} style="color:#ef4444;background:transparent;border:none;cursor:pointer;font-size:13px;">
          Удалить
        </button>
      </div>

      <!-- Мета: привязка к задаче + теги -->
      <div style="padding:8px 14px;border-bottom:1px solid var(--border,#e5e7eb);display:flex;flex-wrap:wrap;gap:10px;align-items:center;">
        <label style="display:flex;align-items:center;gap:6px;font-size:12px;color:var(--text-secondary,#6b7280);">
          Задача:
          <select bind:value={editLinkedTaskId} onchange={saveMeta} style="font-size:12px;max-width:200px;">
            <option value={null}>— не привязана —</option>
            {#each taskStore.activeTasks as t (t.id)}
              <option value={t.id}>{t.title}</option>
            {/each}
          </select>
        </label>
        {#if linkedTask}
          <span style="font-size:11px;background:var(--bg-secondary,#f5f5f5);border:1px solid var(--border,#e5e7eb);
            border-radius:10px;padding:2px 8px;">🔗 {linkedTask.title}</span>
        {/if}

        <div style="display:flex;align-items:center;gap:6px;flex:1;min-width:180px;">
          {#each editTags as tag (tag)}
            <span style="font-size:11px;background:var(--bg-secondary,#f5f5f5);border:1px solid var(--border,#e5e7eb);
              border-radius:10px;padding:2px 8px;display:inline-flex;align-items:center;gap:4px;">
              {tag}
              <button onclick={() => removeTag(tag)}
                style="border:none;background:transparent;padding:0;font-size:12px;line-height:1;cursor:pointer;color:inherit;">×</button>
            </span>
          {/each}
          <input
            bind:value={tagInput}
            onkeydown={onTagKeydown}
            placeholder="+ тег"
            style="font-size:12px;border:none;outline:none;background:transparent;width:70px;"
          />
        </div>
      </div>

      {#if previewMode}
        <div bind:this={previewEl} class="md-preview">{@html previewHtml}</div>
      {:else}
        <textarea
          bind:value={editContent}
          oninput={scheduleSave}
          placeholder="Начните писать... (поддерживается Markdown, чек-листы: - [ ] пункт)"
          style="flex:1;padding:14px;border:none;outline:none;resize:none;font-size:14px;
            line-height:1.6;font-family:inherit;background:transparent;"
        ></textarea>
      {/if}
    {/if}
  </div>
</div>

<style>
  .md-preview {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    font-size: 14px;
    line-height: 1.6;
  }
  .md-preview :global(h1),
  .md-preview :global(h2),
  .md-preview :global(h3) { margin: 0.6em 0 0.3em; }
  .md-preview :global(ul),
  .md-preview :global(ol) { padding-left: 1.4em; margin: 0.4em 0; }
  .md-preview :global(li) { margin: 0.15em 0; }
  .md-preview :global(input[type="checkbox"]) { margin-right: 6px; cursor: pointer; }
  .md-preview :global(code) {
    background: var(--bg-secondary, #f5f5f5);
    padding: 1px 4px;
    border-radius: 4px;
    font-size: 0.9em;
  }
  .md-preview :global(pre) {
    background: var(--bg-secondary, #f5f5f5);
    padding: 10px;
    border-radius: 6px;
    overflow-x: auto;
  }
  .md-preview :global(a) { color: var(--accent, #6366f1); }
  .md-preview :global(blockquote) {
    border-left: 3px solid var(--border, #e0e0e0);
    padding-left: 10px;
    color: var(--text-secondary, #666);
    margin: 0.4em 0;
  }
</style>
