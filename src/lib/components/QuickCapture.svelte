<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit, listen } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";
  import { categoryStore } from "../stores/categories.svelte";
  import { parseClipboardNote } from "../clipboardNote";
  import { parseChecklist, formatChecklist } from "../checklistText";
  import ChecklistEditor from "./ChecklistEditor.svelte";
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
  //
  // v0.9.45: чек-лист стал текстовым полем (как в TaskModal), поэтому «сразу»
  // уточняется — на каждую букву в БД не пишем. Правка уезжает через паузу
  // набора, а также при закрытии окна и по «Сохранить»: мгновенность нужна
  // против потери галочки на Escape, а не буквально на каждый keypress.
  let subs = $state<Subtask[]>([]);
  let subsText = $state("");
  let subsBusy = $state(false);
  let subsTimer: ReturnType<typeof setTimeout> | null = null;
  const SUBS_DEBOUNCE_MS = 600;
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
      subsText = formatChecklist(subs.map((s) => ({ title: s.title, done: s.done })));
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
    // закрытие окна.
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
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? t("Не удалось создать задачу");
    } finally {
      busy = false;
    }
  }

  async function createNote() {
    if ((!noteTitle.trim() && !noteContent.trim()) || busy) return;
    busy = true;
    try {
      await api.createNote({
        title: noteTitle.trim() || t("Без названия"),
        content: noteContent,
      });
      await emit("note-created");
      await getCurrentWindow().hide();
      reset();
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? t("Не удалось создать заметку");
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
      errorMsg = t("Заголовок не может быть пустым");
      return;
    }
    busy = true;
    errorMsg = null;
    try {
      if (pinned.kind === "task") {
        await flushSubs();
        await api.updateTask(pinned.id, { title, description: pinnedText });
        await emit("task-updated");
      } else {
        await api.updateNote(pinned.id, { title, content: pinnedText });
        await emit("note-updated");
      }
      saved = true;
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? t("Не удалось сохранить");
    } finally {
      busy = false;
    }
  }

  // Операции чек-листа сохраняются мгновенно. Общая для всех трёх схема:
  // сначала запрос, потом правка локального списка — при ошибке на экране
  // остаётся то, что реально лежит в БД, а не оптимистично отрисованное.
  // Перечитывать слот целиком после каждого клика не нужно: заголовок и текст
  // могут быть отредактированы прямо сейчас, и перечитывание затёрло бы правку.
  // Правка чек-листа: откладываем запись, пока пользователь печатает. Таймер
  // перезапускается на каждое изменение, поэтому в БД уходит уже готовая
  // строка, а не по букве на подзадачу.
  function scheduleSubsFlush() {
    if (subsTimer) clearTimeout(subsTimer);
    subsTimer = setTimeout(() => { subsTimer = null; flushSubs(); }, SUBS_DEBOUNCE_MS);
  }

  // Тот же позиционный diff, что в TaskModal: i-я строка правит i-ю подзадачу.
  // Локальный subs — то, что реально лежит в БД; при ошибке он не трогается,
  // и следующий flush попробует применить ту же правку заново.
  async function flushSubs() {
    if (subsTimer) { clearTimeout(subsTimer); subsTimer = null; }
    if (!pinned || pinned.kind !== "task" || subsBusy) return;
    const current = parseChecklist(subsText);
    const orig = subs;
    const same =
      current.length === orig.length &&
      current.every((c, i) => c.title === orig[i].title && c.done === orig[i].done);
    if (same) return;
    subsBusy = true;
    errorMsg = null;
    try {
      const next: Subtask[] = [];
      for (let i = current.length; i < orig.length; i++) {
        await api.deleteSubtask(orig[i].id);
      }
      for (let i = 0; i < current.length; i++) {
        const c = current[i];
        const o = orig[i];
        if (!o) {
          const added = await api.addSubtask(pinned.id, c.title);
          if (c.done) await api.toggleSubtask(added.id);
          next.push({ ...added, done: c.done });
        } else {
          if (o.title !== c.title) await api.renameSubtask(o.id, c.title);
          if (o.done !== c.done) await api.toggleSubtask(o.id);
          next.push({ ...o, title: c.title, done: c.done });
        }
      }
      subs = next;
      await emit("task-updated");
    } catch (e) {
      errorMsg = typeof e === "string" ? e : (e as Error)?.message ?? t("Не удалось сохранить подзадачи");
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
    // Отложенную правку чек-листа дописываем ДО скрытия окна: Escape здесь —
    // штатный способ закрыть слот, и потерять на нём набранное нельзя.
    await flushSubs();
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
        <span class="pin-badge">⚡ {pinned.kind === "task" ? t("Задача") : t("Заметка")}</span>
        {#if saved}<span class="pin-saved">{t("Сохранено")}</span>{/if}
      </div>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="pin-title" bind:value={pinnedTitle} placeholder={t("Заголовок...")}
        oninput={() => saved = false} />
      <textarea class="pin-text" bind:value={pinnedText} placeholder={t("Текст... (Ctrl+Enter — сохранить)")}
        rows={pinned.kind === "task" ? 3 : 6} autofocus oninput={() => saved = false}></textarea>

      <!-- Чек-лист — только у задачи: у заметки подзадач не бывает. Правки
           здесь уходят в БД сами (через паузу набора, а также по Escape и
           «Сохранить»), поэтому кнопка «Сохранить» ниже про заголовок и текст. -->
      {#if pinned.kind === "task"}
        <div class="subs">
          <div class="subs-head">
            <span class="subs-label">{t("Подзадачи")}</span>
          </div>
          <ChecklistEditor
            bind:value={subsText}
            placeholder={t("Подзадача на строку (Enter — ещё строка)")}
            onchange={scheduleSubsFlush}
          />
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
          <option value={c.id}>{categoryStore.name(c.id)}</option>
        {/each}
      </select>
      <button class="desc-toggle" onclick={() => showDescription = !showDescription}>
        {showDescription ? "−" : t("+ описание")}
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
  /* Строки чек-листа и счётчик переехали в ChecklistEditor (v0.9.45):
     удаление строки — это удаление текста, отдельный крестик не нужен. */
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
