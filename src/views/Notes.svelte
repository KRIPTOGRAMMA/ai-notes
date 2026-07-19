<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { noteStore } from "../lib/stores/notes.svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { api } from "../lib/api/tauri";
  import { extractWikiLinks } from "../lib/markdown";
  import type { Note, NoteRevision } from "../lib/types";
  type EditorComponent = typeof import("../lib/components/LiveMarkdownEditor.svelte").default;

  let selectedId: string | null = $state(null);
  let dailyKey = $state(0); // отслеживаем dailyRequested
  let editTitle = $state("");
  let editContent = $state("");
  let editTags: string[] = $state([]);
  let editLinkedTaskId: string | null = $state(null);
  let editProjectId: string | null = $state(null);
  let tagInput = $state("");
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;
  let saving = $state(false);
  let renameToast: string | null = $state(null);
  let renameToastTimeout: ReturnType<typeof setTimeout> | null = null;

  const selected = $derived(noteStore.notes.find(n => n.id === selectedId) ?? null);
  const otherTitles = $derived(noteStore.notes.filter(n => n.id !== selectedId).map(n => n.title));

  // Фильтр списка заметок
  let noteFilter = $state("");
  let filterTag = $state("");
  let filterProjectId = $state("");
  const allTags = $derived([...new Set(noteStore.notes.flatMap(n => n.tags))].sort());
  const filteredNotes = $derived(noteStore.notes.filter(n => {
    if (noteFilter && !n.title.toLowerCase().includes(noteFilter.toLowerCase())) return false;
    if (filterTag && !n.tags.includes(filterTag)) return false;
    if (filterProjectId && n.project_id !== filterProjectId) return false;
    return true;
  }));

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
    suppressNextContentSave = true;
    selectedId = note.id;
    editTitle = note.title;
    editContent = note.content;
    editTags = [...note.tags];
    editLinkedTaskId = note.linked_task_id;
    editProjectId = note.project_id;
    linkSuggestions = null;
  }

  // CodeMirror меняет editContent напрямую через bind:value (без oninput-хука),
  // поэтому автосохранение вешаем на $effect. suppressNextContentSave гасит
  // срабатывание, вызванное самим selectNote (программная подмена, не ввод).
  let suppressNextContentSave = false;
  $effect(() => {
    editContent;
    if (suppressNextContentSave) { suppressNextContentSave = false; return; }
    scheduleSave();
  });

  async function openWikiLink(title: string) {
    const existing = findByTitle(title);
    if (existing) {
      selectNote(existing);
      return;
    }
    const created = await noteStore.create({ title, content: "" });
    if (created) selectNote(created);
  }

  async function openDailyNote() {
    const today = new Date();
    const yyyy = today.getFullYear();
    const mm = String(today.getMonth() + 1).padStart(2, "0");
    const dd = String(today.getDate()).padStart(2, "0");
    const title = `${yyyy}-${mm}-${dd}`;
    const existing = findByTitle(title);
    if (existing) { selectNote(existing); return; }
    // Дата вчера
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);
    const yy = yesterday.getFullYear();
    const ym = String(yesterday.getMonth() + 1).padStart(2, "0");
    const yd = String(yesterday.getDate()).padStart(2, "0");
    const created = await noteStore.create({ title, content: `[[${yy}-${ym}-${yd}]]\n\n` });
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

  // Сигнал «открыть заметку дня» (Ctrl+D из другого раздела).
  $effect(() => {
    dailyKey;
    if (noteStore.dailyRequested === 0) return;
    dailyKey = noteStore.dailyRequested;
    openDailyNote();
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
    // Панель версий могла быть открыта на этой же заметке — её ревизии удаляются
    // каскадом на бэкенде; закрываем панель, иначе клик по уже несуществующей
    // ревизии («Восстановить», повторный просмотр) вернёт ошибку с бэкенда.
    revisionsOpen = false;
    viewingRevisionId = null;
    await noteStore.remove(selectedId);
    selectedId = null;
    editTitle = "";
    editContent = "";
    editTags = [];
    editLinkedTaskId = null;
  }

  function formatDate(iso: string) {
    return new Date(iso).toLocaleDateString("ru-RU", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  }

  const linkedTask = $derived(
    editLinkedTaskId ? taskStore.tasks.find(t => t.id === editLinkedTaskId) ?? null : null
  );

  // --- ИИ-автолинковка (v0.6.8): «Предложить связи» ---
  let aiEnabled = $state(false);
  let linkSuggesting = $state(false);
  let linkSuggestions: { noteId: string; titles: string[]; error: string | null } | null = $state(null);

  async function suggestLinks() {
    if (!selectedId) return;
    linkSuggesting = true;
    linkSuggestions = null;
    try {
      await api.aiSuggestLinks(selectedId);
    } catch (e) {
      linkSuggesting = false;
      linkSuggestions = { noteId: selectedId, titles: [], error: String(e) };
    }
  }

  function acceptLinkSuggestion(title: string) {
    const sep = editContent && !editContent.endsWith("\n") ? "\n" : "";
    editContent = `${editContent}${sep}[[${title}]]`; // сохранение — через $effect на editContent
    linkSuggestions = linkSuggestions
      ? { ...linkSuggestions, titles: linkSuggestions.titles.filter(t => t !== title) }
      : null;
  }

  // --- Версии заметки (v0.7.12) ---
  let revisionsOpen = $state(false);
  let revisions: NoteRevision[] = $state([]);
  let viewingRevisionId: string | null = $state(null);
  let viewingRevisionContent = $state("");
  let revisionsBusy = $state(false);

  async function openRevisions() {
    if (!selectedId) return;
    await flushPendingSave();
    revisionsOpen = true;
    viewingRevisionId = null;
    revisions = await api.getNoteRevisions(selectedId).catch(() => []);
  }

  function closeRevisions() {
    revisionsOpen = false;
    viewingRevisionId = null;
  }

  async function viewRevision(rev: NoteRevision) {
    viewingRevisionId = rev.id;
    viewingRevisionContent = await api.getNoteRevisionContent(rev.id).catch(() => "");
  }

  async function restoreRevision(rev: NoteRevision) {
    if (!selectedId) return;
    if (!confirm("Восстановить эту версию? Текущий текст тоже сохранится в версиях.")) return;
    revisionsBusy = true;
    try {
      const updated = await api.restoreNoteRevision(rev.id);
      editContent = updated.content;
      suppressNextContentSave = true;
      await noteStore.load();
      revisionsOpen = false;
    } finally {
      revisionsBusy = false;
    }
  }

  onMount(() => {
    noteStore.load();
    taskStore.load();
    // Капабилити-детект: при выключенном ИИ кнопка «Предложить связи» скрыта
    api.getSettings().then(s => aiEnabled = s.ai_provider !== "none").catch(() => {});
    const unlisteners: UnlistenFn[] = [];
    (async () => {
      unlisteners.push(await listen<{ note_id: string; titles: string[]; error: string | null }>("ai-links", (e) => {
        linkSuggesting = false;
        linkSuggestions = { noteId: e.payload.note_id, titles: e.payload.titles, error: e.payload.error };
      }));
    })();
    return () => unlisteners.forEach(u => u());
  });
</script>

<div class="notes card">
  <!-- Список заметок -->
  <div class="list-pane">
    <div class="list-head">
      <button class="btn-primary btn-sm" style="width:100%;" onclick={newNote}>+ Новая заметка</button>
      <button class="btn-ghost btn-sm" style="width:100%;" onclick={openDailyNote}>📅 Сегодня</button>
      <input class="filter-input" bind:value={noteFilter} placeholder="Поиск..." />
      <div class="filter-row">
        <select bind:value={filterTag} class="filter-select">
          <option value="">Все теги</option>
          {#each allTags as t}
            <option value={t}>#{t}</option>
          {/each}
        </select>
        <select bind:value={filterProjectId} class="filter-select">
          <option value="">Все проекты</option>
          {#each projectStore.active as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
        </select>
      </div>
    </div>

    {#if noteStore.notes.length === 0}
      <div class="empty">Нет заметок</div>
    {:else if filteredNotes.length === 0}
      <div class="empty">Нет заметок по фильтру</div>
    {:else}
      <ul class="note-list">
        {#each filteredNotes as note (note.id)}
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
        {#if aiEnabled}
          <button class="btn-icon" disabled={linkSuggesting} title="ИИ предложит заметки для связи"
            onclick={suggestLinks}>{linkSuggesting ? "…" : "🔗✨"}</button>
        {/if}
        <button class="btn-icon" title="Версии заметки" onclick={openRevisions}>🕐</button>
        <button class="btn-icon btn-danger" title="Удалить заметку" onclick={deleteSelected}>✕</button>
      </div>

      {#if linkSuggestions && linkSuggestions.noteId === selectedId}
        <div class="link-suggest">
          {#if linkSuggestions.error}
            <span class="alert" style="margin:0;">{linkSuggestions.error}</span>
          {:else if linkSuggestions.titles.length === 0}
            <span class="muted">Связей не найдено</span>
          {:else}
            <span class="muted">Связанные:</span>
            {#each linkSuggestions.titles as t (t)}
              <button class="chip link-chip" onclick={() => acceptLinkSuggestion(t)} title="Добавить [[{t}]] в заметку">
                + {t}
              </button>
            {/each}
          {/if}
          <button class="btn-icon" title="Закрыть" onclick={() => linkSuggestions = null}>✕</button>
        </div>
      {/if}

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
        {#key selectedId}
          {#await import("../lib/components/LiveMarkdownEditor.svelte") then { default: Editor }}
            <Editor
              bind:value={editContent}
              placeholder="Начните писать... (Markdown, чек-листы: - [ ] пункт, ссылки: [[заметка]])"
              knownTitles={otherTitles}
              resolveExists={(t) => findByTitle(t) !== null}
              onWikiLinkClick={openWikiLink}
              onSubmitShortcut={() => {}}
            />
          {/await}
        {/key}
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

{#if revisionsOpen}
  <div class="backdrop" role="presentation" onclick={closeRevisions} onkeydown={(e) => e.key === "Escape" && closeRevisions()}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="dialog card revisions-dialog" role="dialog" onclick={(e) => e.stopPropagation()}>
      <h3 class="dialog-title">Версии заметки</h3>

      {#if revisions.length === 0}
        <p class="muted">Ещё нет сохранённых версий — они появляются при правках с интервалом от 10 минут.</p>
      {:else}
        <div class="revisions-body">
          <ul class="revisions-list">
            {#each revisions as rev (rev.id)}
              <li>
                <button class="revision-item" class:active={viewingRevisionId === rev.id} onclick={() => viewRevision(rev)}>
                  <span>{formatDate(rev.created_at)}</span>
                  <span class="muted" style="font-size:11px;">{rev.size} симв.</span>
                </button>
              </li>
            {/each}
          </ul>
          <div class="revision-preview">
            {#if viewingRevisionId}
              <pre>{viewingRevisionContent}</pre>
              <button class="btn-primary btn-sm" disabled={revisionsBusy}
                onclick={() => restoreRevision(revisions.find(r => r.id === viewingRevisionId)!)}>
                {revisionsBusy ? "Восстановление…" : "Восстановить"}
              </button>
            {:else}
              <span class="muted">Выберите версию слева для просмотра</span>
            {/if}
          </div>
        </div>
      {/if}

      <div class="actions">
        <button class="btn-ghost" onclick={closeRevisions}>Закрыть</button>
      </div>
    </div>
  </div>
{/if}

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
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .filter-input {
    font-size: 12px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-primary);
    color: var(--text-primary);
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .filter-input:focus { border-color: var(--accent); }

  .filter-row {
    display: flex;
    gap: 4px;
  }

  .filter-select {
    font-size: 11px;
    flex: 1;
    padding: 2px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-primary);
    color: var(--text-primary);
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

  .link-suggest {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }

  .link-chip {
    border: none;
    cursor: pointer;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .link-chip:hover { background: color-mix(in srgb, var(--accent) 20%, transparent); }

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

  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 16px;
  }

  .dialog {
    width: 100%;
    max-height: 90vh;
    overflow-y: auto;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .revisions-dialog {
    max-width: 640px;
  }

  .dialog-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .revisions-body {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 12px;
    min-height: 280px;
  }

  .revisions-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 340px;
    overflow-y: auto;
  }

  .revision-item {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: transparent;
    cursor: pointer;
    font-size: 12px;
  }

  .revision-item.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .revision-preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .revision-preview pre {
    flex: 1;
    margin: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: auto;
    max-height: 340px;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 12px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }
</style>
