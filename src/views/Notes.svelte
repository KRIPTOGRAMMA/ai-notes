<script lang="ts">
  import { onMount } from "svelte";
  import { noteStore } from "../lib/stores/notes.svelte";
  import type { Note } from "../lib/types";

  let selectedId: string | null = $state(null);
  let editTitle = $state("");
  let editContent = $state("");
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;
  let saving = $state(false);

  const selected = $derived(noteStore.notes.find(n => n.id === selectedId) ?? null);

  function selectNote(note: Note) {
    selectedId = note.id;
    editTitle = note.title;
    editContent = note.content;
  }

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

  async function deleteSelected() {
    if (!selectedId) return;
    await noteStore.remove(selectedId);
    selectedId = null;
    editTitle = "";
    editContent = "";
  }

  function formatDate(iso: string) {
    return new Date(iso).toLocaleDateString("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  }

  noteStore.load();
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
          <li
            onclick={() => selectNote(note)}
            style="padding:10px 12px;cursor:pointer;border-bottom:1px solid var(--border,#e5e7eb);
              background:{selectedId === note.id ? 'var(--accent-light,#eff6ff)' : 'transparent'};"
          >
            <div style="font-size:13px;font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">
              {note.title}
            </div>
            <div style="font-size:11px;color:var(--text-secondary,#6b7280);margin-top:2px;">
              {formatDate(note.updated_at)}
            </div>
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
        <button onclick={deleteSelected} style="color:#ef4444;background:transparent;border:none;cursor:pointer;font-size:13px;">
          Удалить
        </button>
      </div>

      <textarea
        bind:value={editContent}
        oninput={scheduleSave}
        placeholder="Начните писать..."
        style="flex:1;padding:14px;border:none;outline:none;resize:none;font-size:14px;
          line-height:1.6;font-family:inherit;background:transparent;"
      ></textarea>
    {/if}
  </div>
</div>
