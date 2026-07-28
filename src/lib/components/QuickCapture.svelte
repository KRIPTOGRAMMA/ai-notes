<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit, listen } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";
  import { categoryStore } from "../stores/categories.svelte";
  import { parseClipboardNote } from "../clipboardNote";
  import { applyCachedTheme } from "../theme";
  import type { PinnedItem, Subtask } from "../types";
  import { t } from "../i18n.svelte";
  import "../../app.css";

  // "pinned" (v0.9.33) — полноценный третий режим, в отличие от "clipboard":
  // тот схлопывался в "note", потому что тоже создавал заметку. Здесь окно
  // ничего не создаёт, а правит существующее, поэтому и форма своя.
  type Mode = "task" | "note" | "pinned";
  let mode = $state<Mode>("task");

  // Закреплённое: null — слот пуст (не закрепляли, или объект удалён).
  let pinned = $state<PinnedItem | null>(null);
  let pinnedTitle = $state("");
  let pinnedText = $state("");
  let saved = $state(false);
  // v0.9.34: чек-лист закреплённой задачи. В отличие от TaskModal, где правки
  // копятся и уезжают diff'ом по «Сохранить», здесь каждый клик уходит в БД
  // сразу — слот открывают, чтобы отметить сделанное и закрыть, и потерять
  // галочку на Escape было бы обиднее, чем в форме редактирования.
  let subs = $state<Subtask[]>([]);
  let newSub = $state("");
  let subsBusy = $state(false);
  const subsDone = $derived(subs.filter((s) => s.done).length);
  // v0.9.26: подсказка «текст взят из буфера» — только для режима clipboard,
  // снимается при первой же правке, чтобы не висеть над отредактированным
  // текстом. Сам "clipboard" в Mode не входит: это не третья вкладка, а
  // предзаполненная заметка, поэтому в UI он схлопывается в "note".
  let fromClipboard = $state(false);

  // Задача
  let title = $state("");
  let description = $state("");
  let priority = $state("Medium");
  let category = $state("Other"); // фолбэк-категория существует всегда
  let showDescription = $state(false);

  // Заметка
  let noteTitle = $state("");
  let noteContent = $state("");

  let errorMsg: string | null = $state(null);
  let busy = $state(false);

  applyCachedTheme();

  // Режим clipboard раскрывается в заметку, предзаполненную буфером обмена.
  // Пустой буфер (или картинка/файл в нём) — не ошибка: открывается обычная
  // пустая заметка, как по Ctrl+Shift+M.
  async function applyMode(m: string) {
    if (m === "pinned") {
      mode = "pinned";
      fromClipboard = false;
      saved = false;
      // Слот читается при каждом открытии, а не кэшируется: закреплённое
      // могли отредактировать в главном окне или удалить.
      pinned = await api.getPinnedItem().catch(() => null);
      pinnedTitle = pinned?.title ?? "";
      pinnedText = pinned?.text ?? "";
      subs = pinned?.subtasks ?? [];
      newSub = "";
      return;
    }
    if (m !== "clipboard") {
      mode = m === "note" ? "note" : "task";
      fromClipboard = false;
      return;
    }
    mode = "note";
    const text = await api.readClipboardText().catch(() => "");
    const parsed = parseClipboardNote(text);
    if (!parsed) {
      fromClipboard = false;
      return;
    }
    noteTitle = parsed.title;
    noteContent = parsed.content;
    fromClipboard = true;
  }

  onMount(() => {
    // Начальный режим — из managed-state (покрывает случай, когда окно уже было
    // смонтировано до эмита события).
    api.getQuickMode().then(applyMode).catch(() => {});
    categoryStore.load();
    // Живая смена режима, пока окно открыто.
    const un = listen<string>("quick-mode", (e) => { applyMode(e.payload); });
    return () => { un.then((f) => f()); };
  });

  function reset() {
    title = ""; description = ""; priority = "Medium"; category = "Other"; showDescription = false;
    noteTitle = ""; noteContent = "";
    errorMsg = null;
    fromClipboard = false;
    // Сам слот (pinned) и его чек-лист не сбрасываем: reset чистит черновики
    // создания, а закреплённое — не черновик, оно живёт в БД и переживает
    // закрытие окна. Чистится только недописанная строка новой подзадачи.
    newSub = "";
    saved = false;
  }

  async function createTask() {
    if (!title.trim() || busy) return;
    busy = true;
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
    } finally {
      busy = false;
    }
  }

  async function createNote() {
    if ((!noteTitle.trim() && !noteContent.trim()) || busy) return;
    busy = true;
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
    } finally {
      busy = false;
    }
  }

  // Правка закреплённого. В отличие от createTask/createNote окно НЕ прячется
  // после сохранения: слот — это то, к чему возвращаются, и дописывать в него
  // обычно хочется несколькими подходами. Вместо закрытия — пометка «Сохранено».
  async function savePinned() {
    if (!pinned || busy) return;
    const title = pinnedTitle.trim();
    // Пустой заголовок отклоняем: у задачи он обязателен на бэкенде, а у
    // заметки превратился бы в безымянную строку в списке.
    if (!title) {
      errorMsg = "Заголовок не может быть пустым";
      return;
    }
    busy = true;
    errorMsg = null;
    try {
      if (pinned.kind === "task") {
        await api.updateTask(pinned.id, { title, description: pinnedText });
        await emit("task-updated");
      } else {
        await api.updateNote(pinned.id, { title, content: pinnedText });
        await emit("note-updated");
      }
      saved = true;
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось сохранить";
    } finally {
      busy = false;
    }
  }

  // Операции чек-листа сохраняются мгновенно. Общая для всех трёх схема:
  // сначала запрос, потом правка локального списка — при ошибке на экране
  // остаётся то, что реально лежит в БД, а не оптимистично отрисованное.
  // Перечитывать слот целиком после каждого клика не нужно: заголовок и текст
  // могут быть отредактированы прямо сейчас, и перечитывание затёрло бы правку.
  async function toggleSub(s: Subtask) {
    if (subsBusy) return;
    subsBusy = true;
    errorMsg = null;
    try {
      await api.toggleSubtask(s.id);
      subs = subs.map((x) => (x.id === s.id ? { ...x, done: !x.done } : x));
      await emit("task-updated");
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось отметить подзадачу";
    } finally {
      subsBusy = false;
    }
  }

  async function addSub() {
    const title = newSub.trim();
    if (!title || !pinned || pinned.kind !== "task" || subsBusy) return;
    subsBusy = true;
    errorMsg = null;
    try {
      const created = await api.addSubtask(pinned.id, title);
      subs = [...subs, created];
      newSub = "";
      await emit("task-updated");
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось добавить подзадачу";
    } finally {
      subsBusy = false;
    }
  }

  async function removeSub(s: Subtask) {
    if (subsBusy) return;
    subsBusy = true;
    errorMsg = null;
    try {
      await api.deleteSubtask(s.id);
      subs = subs.filter((x) => x.id !== s.id);
      await emit("task-updated");
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? "Не удалось удалить подзадачу";
    } finally {
      subsBusy = false;
    }
  }

  function submit() {
    if (mode === "pinned") savePinned();
    else if (mode === "task") createTask();
    else createNote();
  }

  async function cancel() {
    await getCurrentWindow().hide();
    reset();
  }

  function onKeydown(e: KeyboardEvent) {
    // Ctrl+Tab переключает вкладки создания. В режиме правки закреплённого
    // вкладок нет — уводить оттуда в форму создания значило бы бросить
    // несохранённую правку, поэтому режим игнорирует переключение.
    if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      if (mode !== "pinned") mode = mode === "task" ? "note" : "task";
      return;
    }
    if (e.key === "Escape") { cancel(); return; }
    // Enter создаёт: для задачи — в любом поле; для заметки — только с Ctrl
    // (обычный Enter в textarea переносит строку). Правка закреплённого — как
    // заметка: там тоже многострочный текст.
    if (e.key === "Enter" && !e.shiftKey) {
      if (mode === "task") { e.preventDefault(); submit(); }
      else if (e.ctrlKey) { e.preventDefault(); submit(); }
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="container">
  {#if mode !== "pinned"}
    <div class="tabs">
      <div class="seg">
        <button class:active={mode === "task"} onclick={() => mode = "task"}>{t("Задача")}</button>
        <button class:active={mode === "note"} onclick={() => mode = "note"}>{t("Заметка")}</button>
      </div>
      <span style="flex:1;"></span>
      <kbd>Ctrl Tab</kbd>
    </div>
  {/if}

  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}

  {#if mode === "pinned"}
    {#if pinned}
      <div class="pin-head">
        <span class="pin-badge">⚡ {pinned.kind === "task" ? "Задача" : "Заметка"}</span>
        {#if saved}<span class="pin-saved">{t("Сохранено")}</span>{/if}
      </div>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="pin-title" bind:value={pinnedTitle} placeholder={t("Заголовок...")}
        oninput={() => saved = false} />
      <textarea class="pin-text" bind:value={pinnedText} placeholder={t("Текст... (Ctrl+Enter — сохранить)")}
        rows={pinned.kind === "task" ? 3 : 6} autofocus oninput={() => saved = false}></textarea>

      <!-- Чек-лист — только у задачи: у заметки подзадач не бывает. Правки
           здесь уходят в БД сразу, поэтому кнопка «Сохранить» ниже их не
           касается — она про заголовок и текст. -->
      {#if pinned.kind === "task"}
        <div class="subs">
          <div class="subs-head">
            <span class="subs-label">{t("Подзадачи")}</span>
            {#if subs.length}<span class="subs-count">{subsDone} / {subs.length}</span>{/if}
          </div>
          {#each subs as s (s.id)}
            <div class="sub-row" class:done={s.done}>
              <input type="checkbox" checked={s.done} disabled={subsBusy}
                onchange={() => toggleSub(s)} aria-label={s.title} />
              <span class="sub-title">{s.title}</span>
              <button class="sub-del" onclick={() => removeSub(s)} disabled={subsBusy}
                title={t("Удалить подзадачу")} aria-label="Удалить подзадачу {s.title}">✕</button>
            </div>
          {/each}
          <input class="sub-new" bind:value={newSub} placeholder={t("+ подзадача (Enter)")}
            disabled={subsBusy}
            onkeydown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); addSub(); }
            }} />
        </div>
      {/if}

      <div class="buttons">
        <button class="btn-ghost" onclick={cancel}>{t("Закрыть")}</button>
        <button class="btn-primary" onclick={savePinned} disabled={busy || !pinnedTitle.trim()}>
          {t("Сохранить")}
        </button>
      </div>
    {:else}
      <!-- Пустой слот — не ошибка: пользователь ещё ничего не закреплял, либо
           закреплённое удалено. Объясняем, как закрепить, вместо пустого окна. -->
      <div class="pin-empty">
        <p class="pin-empty-title">{t("⚡ Слот пуст")}</p>
        <p class="pin-empty-hint">
          {t("Закрепите задачу или заметку кнопкой-молнией в списке — этот хоткей будет открывать её сразу на правку.")}
        </p>
      </div>
      <div class="buttons">
        <button class="btn-ghost" onclick={cancel}>{t("Закрыть")}</button>
      </div>
    {/if}
  {:else if mode === "task"}
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={title} placeholder={t("Название задачи...")} autofocus />

    <div class="row">
      <select bind:value={priority}>
        <option value="Low">{t("Низкий")}</option>
        <option value="Medium">{t("Средний")}</option>
        <option value="High">{t("Высокий")}</option>
        <option value="Critical">{t("Критический")}</option>
      </select>
      <select bind:value={category}>
        {#each categoryStore.categories as c (c.id)}
          <option value={c.id}>{c.name}</option>
        {/each}
      </select>
      <button class="desc-toggle" onclick={() => showDescription = !showDescription}>
        {showDescription ? "−" : "+ описание"}
      </button>
    </div>

    {#if showDescription}
      <textarea bind:value={description} placeholder={t("Описание...")} rows="2"></textarea>
    {/if}

    <div class="buttons">
      <button class="btn-ghost" onclick={cancel}>{t("Отмена")}</button>
      <button class="btn-primary" onclick={createTask} disabled={!title.trim()}>{t("Создать")}</button>
    </div>
  {:else}
    {#if fromClipboard}
      <p class="clip-hint">{t("Текст из буфера обмена — можно поправить перед сохранением")}</p>
    {/if}
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={noteTitle} placeholder={t("Заголовок заметки...")} autofocus
      oninput={() => fromClipboard = false} />
    <textarea bind:value={noteContent} placeholder={t("Текст заметки... (Ctrl+Enter — сохранить)")} rows="3"
      oninput={() => fromClipboard = false}></textarea>

    <div class="buttons">
      <button class="btn-ghost" onclick={cancel}>{t("Отмена")}</button>
      <button class="btn-primary" onclick={createNote} disabled={!noteTitle.trim() && !noteContent.trim()}>{t("Создать")}</button>
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
  .error {
    font-size: 12px;
    color: var(--danger);
    margin: 0;
  }
  .clip-hint {
    font-size: 11px;
    color: var(--text-muted, #888);
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
  /* v0.9.33: правка закреплённого. Форма создания — «пустой бланк»,
     здесь наоборот важно с первого взгляда понять, что правится уже
     существующее, поэтому шапка с типом и акцентная рамка вокруг текста. */
  .pin-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pin-badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    letter-spacing: 0.02em;
  }
  .pin-saved {
    font-size: 11px;
    color: var(--text-muted, #888);
    margin-left: auto;
  }
  .pin-title {
    font-weight: 600;
  }
  .pin-text {
    flex: 1;
    resize: none;
    font-size: 13px;
    border-left: 2px solid var(--accent);
  }
  /* v0.9.34: чек-лист в слоте. Отделён от текста заголовком с счётчиком —
     без него две группы полей сливаются в одну простыню. */
  .subs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    overflow-y: auto;
  }
  .subs-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 2px;
  }
  .subs-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted, #888);
  }
  .subs-count {
    font-size: 11px;
    color: var(--text-muted, #888);
    margin-left: auto;
  }
  .sub-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .sub-row input[type="checkbox"] {
    flex-shrink: 0;
    margin: 0;
    cursor: pointer;
  }
  .sub-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub-row.done .sub-title {
    text-decoration: line-through;
    color: var(--text-muted, #888);
  }
  /* Крестик проявляется по наведению на строку: в списке из пяти подзадач
     пять постоянных крестиков читаются как основное действие, хотя удаление
     здесь — редкое. */
  .sub-del {
    flex-shrink: 0;
    padding: 0 4px;
    border: none;
    background: transparent;
    color: var(--text-muted, #888);
    cursor: pointer;
    opacity: 0;
    font-size: 12px;
    line-height: 1;
  }
  .sub-row:hover .sub-del,
  .sub-del:focus-visible {
    opacity: 1;
  }
  .sub-del:hover {
    color: var(--danger);
  }
  .sub-new {
    font-size: 13px;
    margin-top: 2px;
  }
  .pin-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    text-align: center;
  }
  .pin-empty-title {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .pin-empty-hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted, #888);
    line-height: 1.4;
  }
</style>
