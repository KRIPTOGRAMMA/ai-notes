<script lang="ts">
  import { onMount, tick } from "svelte";
  import { noteStore } from "../lib/stores/notes.svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { api } from "../lib/api/tauri";
  import { renderMarkdown, toggleTaskListItem, extractWikiLinks } from "../lib/markdown";
  import type { Note } from "../lib/types";

  let selectedId: string | null = $state(null);
  let editTitle = $state("");
  let editContent = $state("");
  let editTags: string[] = $state([]);
  let editLinkedTaskId: string | null = $state(null);
  let editProjectId: string | null = $state(null);
  let tagInput = $state("");
  let previewMode = $state(false);
  let previewEl: HTMLDivElement | undefined = $state();
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;
  let saving = $state(false);
  let renameToast: string | null = $state(null);
  let renameToastTimeout: ReturnType<typeof setTimeout> | null = null;

  const selected = $derived(noteStore.notes.find(n => n.id === selectedId) ?? null);
  const previewHtml = $derived(renderMarkdown(editContent));

  // Заметки, ссылающиеся на текущую через [[название]] (без учёта регистра).
  const backlinks = $derived.by<Note[]>(() => {
    if (!selectedId) return [];
    const title = editTitle.trim().toLowerCase();
    if (!title) return [];
    return noteStore.notes.filter(n =>
      n.id !== selectedId && extractWikiLinks(n.content).some(l => l.toLowerCase() === title)
    );
  });

  function findByTitle(title: string): Note | null {
    const key = title.trim().toLowerCase();
    return noteStore.notes.find(n => n.title.trim().toLowerCase() === key) ?? null;
  }

  // Пишет title/content и, если название реально изменилось, обновляет
  // [[ссылки]] в остальных заметках (v0.6.7). oldTitle берём из stale-snapshot
  // (selected.title до этого save) — не из editTitle, который уже новый.
  async function persistNote(id: string, oldTitle: string, newTitle: string, content: string) {
    await noteStore.update(id, { title: newTitle, content });
    const trimmed = newTitle.trim();
    if (trimmed && trimmed.toLowerCase() !== oldTitle.trim().toLowerCase()) {
      const count = await api.renameNoteLinks(oldTitle, trimmed);
      if (count > 0) {
        await noteStore.load();
        if (renameToastTimeout) clearTimeout(renameToastTimeout);
        renameToast = `Обновлено ссылок: ${count}`;
        renameToastTimeout = setTimeout(() => { renameToast = null; }, 4000);
      }
    }
  }

  // Отложенное сохранение нельзя терять при смене заметки: сбрасываем таймер
  // и пишем сразу, пока selectedId/editContent ещё указывают на старую.
  async function flushPendingSave() {
    if (!saveTimeout) return;
    clearTimeout(saveTimeout);
    saveTimeout = null;
    if (selectedId) {
      const before = selected?.title ?? editTitle;
      await persistNote(selectedId, before, editTitle, editContent);
    }
    saving = false;
  }

  async function selectNote(note: Note) {
    await flushPendingSave();
    selectedId = note.id;
    editTitle = note.title;
    editContent = note.content;
    editTags = [...note.tags];
    editLinkedTaskId = note.linked_task_id;
    editProjectId = note.project_id;
    previewMode = false;
    acOpen = false;
  }

  async function openWikiLink(title: string) {
    const existing = findByTitle(title);
    if (existing) {
      selectNote(existing);
      return;
    }
    const created = await noteStore.create({ title, content: "" });
    if (created) selectNote(created);
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
    const id = selectedId;
    const before = selected?.title ?? editTitle;
    saveTimeout = setTimeout(async () => {
      await persistNote(id, before, editTitle, editContent);
      saving = false;
    }, 800);
  }

  // Теги и привязка сохраняются сразу (без дебаунса).
  async function saveMeta() {
    if (!selectedId) return;
    await noteStore.update(selectedId, {
      tags: editTags,
      linked_task_id: editLinkedTaskId,
      project_id: editProjectId,
    });
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
    // Отложенное сохранение удаляемой заметки не нужно — просто гасим таймер.
    if (saveTimeout) { clearTimeout(saveTimeout); saveTimeout = null; }
    saving = false;
    acOpen = false;
    await noteStore.remove(selectedId);
    selectedId = null;
    editTitle = "";
    editContent = "";
    editTags = [];
    editLinkedTaskId = null;
  }

  // --- Автодополнение по "[[" в редакторе ---
  let textareaEl: HTMLTextAreaElement | undefined = $state();
  let acOpen = $state(false);
  let acItems: string[] = $state([]);
  let acIndex = $state(0);
  let acPos = $state({ left: 0, top: 0 });
  let acStart = 0; // позиция в editContent сразу после "[["

  // Координаты каретки в textarea: зеркальный div с той же типографикой,
  // маркер-span на месте каретки даёт offsetTop/offsetLeft.
  function caretCoords(ta: HTMLTextAreaElement): { left: number; top: number } {
    const mirror = document.createElement("div");
    const s = getComputedStyle(ta);
    for (const p of [
      "font-family", "font-size", "font-weight", "line-height", "letter-spacing",
      "padding-top", "padding-right", "padding-bottom", "padding-left",
    ]) {
      mirror.style.setProperty(p, s.getPropertyValue(p));
    }
    mirror.style.position = "absolute";
    mirror.style.visibility = "hidden";
    mirror.style.whiteSpace = "pre-wrap";
    mirror.style.overflowWrap = "break-word";
    mirror.style.width = `${ta.clientWidth}px`;
    mirror.textContent = ta.value.slice(0, ta.selectionStart);
    const marker = document.createElement("span");
    marker.textContent = "​";
    mirror.appendChild(marker);
    ta.parentElement!.appendChild(mirror);
    const lineHeight = parseFloat(s.lineHeight) || 21;
    const pos = {
      left: Math.min(marker.offsetLeft, ta.clientWidth - 180),
      top: marker.offsetTop + lineHeight - ta.scrollTop,
    };
    mirror.remove();
    return pos;
  }

  function checkAutocomplete() {
    if (!textareaEl) { acOpen = false; return; }
    const caret = textareaEl.selectionStart;
    const m = /\[\[([^\[\]\n]*)$/.exec(editContent.slice(0, caret));
    if (!m) { acOpen = false; return; }
    const q = m[1].toLowerCase();
    const items = noteStore.notes
      .filter(n => n.id !== selectedId && n.title.toLowerCase().includes(q))
      .map(n => n.title)
      .slice(0, 8);
    if (items.length === 0) { acOpen = false; return; }
    acStart = caret - m[1].length;
    acItems = items;
    acIndex = 0;
    acPos = caretCoords(textareaEl);
    acOpen = true;
  }

  function acPick(title: string) {
    if (!textareaEl) return;
    const caret = textareaEl.selectionStart;
    const closing = editContent.slice(caret).startsWith("]]") ? "" : "]]";
    editContent = editContent.slice(0, acStart) + title + closing + editContent.slice(caret);
    acOpen = false;
    const newPos = acStart + title.length + 2; // каретка после "]]"
    tick().then(() => {
      textareaEl?.focus();
      textareaEl?.setSelectionRange(newPos, newPos);
    });
    scheduleSave();
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if (!acOpen) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      acIndex = (acIndex + 1) % acItems.length;
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      acIndex = (acIndex - 1 + acItems.length) % acItems.length;
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      acPick(acItems[acIndex]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      acOpen = false;
    }
  }

  // Интерактивные чек-листы: после рендера превью снимаем disabled с чекбоксов
  // и по клику переключаем соответствующую строку в самом markdown-тексте.
  // Здесь же оживляем вики-ссылки: клик открывает заметку (или создаёт её),
  // несуществующие цели помечаются классом missing.
  $effect(() => {
    // зависимости: перечитываем при смене html/режима/списка заметок
    previewHtml; previewMode; noteStore.notes;
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
      const links = previewEl.querySelectorAll<HTMLAnchorElement>("a.wikilink");
      links.forEach(a => {
        const target = a.dataset.wikilink ?? "";
        const exists = findByTitle(target) !== null;
        a.classList.toggle("missing", !exists);
        a.title = exists ? target : `Создать «${target}»`;
        a.onclick = (e) => {
          e.preventDefault();
          openWikiLink(target);
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

<div class="notes card">
  <!-- Список заметок -->
  <div class="list-pane">
    <div class="list-head">
      <button class="btn-primary btn-sm" style="width:100%;" onclick={newNote}>+ Новая заметка</button>
    </div>

    {#if noteStore.notes.length === 0}
      <div class="empty">Нет заметок</div>
    {:else}
      <ul class="note-list">
        {#each noteStore.notes as note (note.id)}
          <li>
            <button class="note-item" class:active={selectedId === note.id} onclick={() => selectNote(note)}>
              <div class="note-title">{note.title}</div>
              <div class="note-date">{formatDate(note.updated_at)}</div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <!-- Редактор -->
  <div class="editor-pane">
    {#if !selected}
      <div class="empty" style="margin:auto;">Выберите заметку или создайте новую</div>
    {:else}
      <div class="editor-head">
        <input class="title-input" bind:value={editTitle} oninput={scheduleSave} placeholder="Название" />
        {#if saving}
          <span class="muted" style="font-size:11px;">Сохранение…</span>
        {/if}
        {#if renameToast}
          <span class="rename-toast">{renameToast}</span>
        {/if}
        <div class="seg">
          <button class:active={!previewMode} onclick={() => previewMode = false}>Редактировать</button>
          <button class:active={previewMode} onclick={() => previewMode = true}>Превью</button>
        </div>
        <button class="btn-icon btn-danger" title="Удалить заметку" onclick={deleteSelected}>✕</button>
      </div>

      <!-- Мета: привязка к задаче + теги -->
      <div class="editor-meta">
        <label class="meta-label">
          Задача:
          <select bind:value={editLinkedTaskId} onchange={saveMeta}>
            <option value={null}>— не привязана —</option>
            {#each taskStore.activeTasks as t (t.id)}
              <option value={t.id}>{t.title}</option>
            {/each}
          </select>
        </label>
        {#if projectStore.projects.length > 0}
          <label class="meta-label">
            Проект:
            <select bind:value={editProjectId} onchange={saveMeta}>
              <option value={null}>— без проекта —</option>
              {#each projectStore.active as p (p.id)}
                <option value={p.id}>{p.name}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if linkedTask}
          <span class="chip">🔗 {linkedTask.title}</span>
        {/if}

        <div class="tags">
          {#each editTags as tag (tag)}
            <span class="chip chip-tag">
              #{tag}
              <button class="tag-remove" onclick={() => removeTag(tag)}>×</button>
            </span>
          {/each}
          <input class="tag-input" bind:value={tagInput} onkeydown={onTagKeydown} placeholder="+ тег" />
        </div>
      </div>

      <div class="editor-body">
        {#if previewMode}
          <div bind:this={previewEl} class="md-preview">{@html previewHtml}</div>
        {:else}
          <textarea
            class="content-input"
            bind:this={textareaEl}
            bind:value={editContent}
            oninput={() => { scheduleSave(); checkAutocomplete(); }}
            onkeydown={onEditorKeydown}
            onclick={checkAutocomplete}
            onblur={() => acOpen = false}
            placeholder="Начните писать... (Markdown, чек-листы: - [ ] пункт, ссылки: [[заметка]])"
          ></textarea>
          {#if acOpen}
            <div class="ac-panel" style="left:{acPos.left}px; top:{acPos.top}px;">
              {#each acItems as item, i (item)}
                <button
                  class="ac-item"
                  class:active={i === acIndex}
                  onmousedown={(e) => { e.preventDefault(); acPick(item); }}
                >{item}</button>
              {/each}
            </div>
          {/if}
        {/if}
      </div>

      {#if backlinks.length > 0}
        <div class="backlinks">
          <span class="backlinks-label">Ссылаются сюда:</span>
          {#each backlinks as b (b.id)}
            <button class="backlink chip" onclick={() => selectNote(b)}>{b.title}</button>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .notes {
    display: flex;
    height: 100%;
    overflow: hidden;
  }

  .list-pane {
    width: 210px;
    min-width: 170px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg-secondary);
  }

  .list-head {
    padding: 8px;
    border-bottom: 1px solid var(--border);
  }

  .note-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .note-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 8px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
  }

  .note-item:hover { background: var(--bg-hover); }

  .note-item.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .note-title {
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .note-item.active .note-title { color: var(--accent); }

  .note-date {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 1px;
  }

  .editor-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .title-input {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
    border: none;
    outline: none;
    background: transparent;
    padding: 4px 0;
  }
  .title-input:focus { outline: none; }

  .rename-toast {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
    white-space: nowrap;
  }

  .editor-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }

  .meta-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .meta-label select {
    font-size: 12px;
    max-width: 200px;
    padding: 2px 6px;
  }

  .tags {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 160px;
    flex-wrap: wrap;
  }

  .tag-remove {
    border: none;
    background: transparent;
    padding: 0;
    font-size: 12px;
    line-height: 1;
    color: inherit;
  }

  .tag-input {
    font-size: 12px;
    border: none;
    outline: none;
    background: transparent;
    width: 70px;
    padding: 2px 4px;
  }
  .tag-input:focus { outline: none; }

  .editor-body {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .ac-panel {
    position: absolute;
    z-index: 10;
    min-width: 160px;
    max-width: 280px;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    padding: 3px;
    display: flex;
    flex-direction: column;
  }

  .ac-item {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    border-radius: calc(var(--radius) - 2px);
    padding: 5px 8px;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ac-item:hover { background: var(--bg-hover); }
  .ac-item.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }

  .backlinks {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 12px;
    border-top: 1px solid var(--border);
  }

  .backlinks-label {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .backlink {
    border: none;
    cursor: pointer;
    color: var(--accent);
  }
  .backlink:hover { text-decoration: underline; }

  .content-input {
    flex: 1;
    padding: 12px 14px;
    border: none;
    outline: none;
    resize: none;
    font-size: 13px;
    line-height: 1.6;
    font-family: inherit;
    background: transparent;
  }
  .content-input:focus { outline: none; }

  .md-preview {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    font-size: 13px;
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
    background: var(--bg-secondary);
    padding: 1px 4px;
    border-radius: 4px;
    font-size: 0.9em;
  }
  .md-preview :global(pre) {
    background: var(--bg-secondary);
    padding: 10px;
    border-radius: var(--radius);
    overflow-x: auto;
  }
  .md-preview :global(a) { color: var(--accent); }
  .md-preview :global(a.wikilink) {
    text-decoration: none;
    border-bottom: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .md-preview :global(a.wikilink.missing) {
    color: var(--text-secondary);
    border-bottom-style: dashed;
  }
  .md-preview :global(blockquote) {
    border-left: 3px solid var(--border);
    padding-left: 10px;
    color: var(--text-secondary);
    margin: 0.4em 0;
  }
</style>
