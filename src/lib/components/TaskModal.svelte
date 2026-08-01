<script lang="ts">
  import type { Task, CreateTaskPayload, UpdateTaskPayload, Priority, Category, Recurrence, RecurrenceUnit, TaskStatus, ChecklistTemplate } from "../types";
  import ChecklistEditor from "./ChecklistEditor.svelte";
  import { parseChecklist, formatChecklist } from "../checklistText";
  import { api } from "../api/tauri";
  import { projectStore } from "../stores/projects.svelte";
  import { categoryStore } from "../stores/categories.svelte";
  import { statusStore } from "../stores/statuses.svelte";
  import { taskStore } from "../stores/tasks.svelte";
  import { t } from "../i18n.svelte";

  type Props = {
    task?: Task | null;
    initialDeadline?: string | null; // префилл дедлайна при создании (формат datetime-local)
    initialStatus?: TaskStatus; // префилл статуса при создании (Канбан: колонка задаёт статус)
    // Возвращает созданную задачу (create-режим) — модалка дописывает к ней
    // подзадачи из инлайн-чеклиста; в edit-режиме возврат не используется.
    onSave: (data: CreateTaskPayload | UpdateTaskPayload) => Promise<Task | null | void>;
    onClose: () => void;
  };

  let { task = null, initialDeadline = null, initialStatus = "Todo", onSave, onClose }: Props = $props();

  const isEdit = !!task;

  // Кандидаты в блокеры (v0.9.56): открытые задачи, кроме самой этой и уже
  // добавленных. Циклы бэкенд отвергает сам — здесь их не фильтруем, иначе
  // пришлось бы тянуть весь граф зависимостей во фронтенд ради подсказки.
  // Блокеры держим в своём состоянии, а не читаем из пропа: taskStore.load()
  // после правки создаёт НОВЫЕ объекты задач, и проп остаётся указывать на
  // старый — список в открытой модалке не обновился бы до переоткрытия.
  let blockedBy = $state(task?.blocked_by ?? []);

  const candidateBlockers = $derived(
    taskStore.tasks.filter(c =>
      c.id !== task?.id &&
      !c.hidden &&
      !blockedBy.some(b => b.id === c.id),
    ),
  );

  async function addBlocker(select: HTMLSelectElement) {
    const blockerId = select.value;
    // Сбрасываем сразу: выбор — это действие, а не состояние поля, иначе в
    // селекте останется висеть уже добавленный блокер.
    select.value = "";
    if (!blockerId || !task) return;
    await taskStore.addDependency(task.id, blockerId);
    blockedBy = taskStore.tasks.find(x => x.id === task!.id)?.blocked_by ?? [];
  }

  async function removeBlocker(blockerId: string) {
    if (!task) return;
    await taskStore.removeDependency(task.id, blockerId);
    blockedBy = taskStore.tasks.find(x => x.id === task!.id)?.blocked_by ?? [];
  }

  // Модалку открывают и разделы, не грузившие категории/статусы (Календарь)
  if (categoryStore.categories.length === 0) categoryStore.load();
  if (statusStore.statuses.length === 0) statusStore.load();

  let title = $state(task?.title ?? "");
  let description = $state(task?.description ?? "");
  let status = $state<TaskStatus>(task?.status ?? initialStatus);
  let priority = $state<Priority>(task?.priority ?? "Medium");
  // "Other" — фолбэк-категория, существует всегда (в отличие от Work — её можно удалить)
  let category = $state<Category>(task?.category ?? "Other");
  let tagsInput = $state((task?.tags ?? []).join(", "));
  let totalTaskMins = $state(0);

  if (task) {
    api.getTaskSeconds(task.id).then(s => totalTaskMins = Math.round(s / 60)).catch(() => {});
  }
  // "" = без проекта; в патче пустая строка отвязывает
  let projectId = $state(task?.project_id ?? "");

  // datetime-local работает в локальном времени. toISOString() дал бы UTC —
  // тогда каждое открытие+сохранение сдвигало бы дедлайн на смещение пояса.
  function toLocalInput(iso: string): string {
    const d = new Date(iso);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  let deadline = $state(task?.deadline ? toLocalInput(task.deadline) : (initialDeadline ?? ""));

  type RecurrenceKey = "None" | "Hourly" | "Daily" | "Weekly" | "Custom" | "Weekdays";

  function initRecurrenceKey(): RecurrenceKey {
    const r = task?.recurrence;
    if (!r || r === "None") return "None";
    if (r === "Hourly") return "Hourly";
    if (r === "Daily") return "Daily";
    if (r === "Weekly") return "Weekly";
    if (typeof r === "object" && r !== null && "Weekdays" in r) return "Weekdays";
    return "Custom";
  }

  let recurrenceKey = $state<RecurrenceKey>(initRecurrenceKey());
  function initCustomN(): number {
    const r = task?.recurrence;
    if (typeof r === "object" && r !== null && "Custom" in r) return r.Custom[0];
    return 1;
  }
  function initCustomUnit(): RecurrenceUnit {
    const r = task?.recurrence;
    if (typeof r === "object" && r !== null && "Custom" in r) return r.Custom[1];
    return "Hours";
  }

  let customN = $state(initCustomN());
  let customUnit = $state<RecurrenceUnit>(initCustomUnit());

  // Дни недели для Recurrence::Weekdays — тот же паттерн, что days_mask у
  // рутин (RoutinesModal.svelte): бит 0 = Пн ... бит 6 = Вс.
  const WEEKDAY_LABELS = $derived([t("Пн"), t("Вт"), t("Ср"), t("Чт"), t("Пт"), t("Сб"), t("Вс")]);
  function initWeekdays(): boolean[] {
    const r = task?.recurrence;
    if (typeof r === "object" && r !== null && "Weekdays" in r) {
      return WEEKDAY_LABELS.map((_, i) => (r.Weekdays & (1 << i)) !== 0);
    }
    return WEEKDAY_LABELS.map(() => false);
  }
  let weekdays = $state<boolean[]>(initWeekdays());
  function weekdaysMask(): number {
    return weekdays.reduce((acc, on, i) => acc | (on ? 1 << i : 0), 0);
  }

  let saving = $state(false);
  let error = $state("");

  // --- Чек-лист подзадач одним текстовым полем (v0.8.3 → переписано в
  // v0.9.45). Строка = подзадача, префикс `[x]`/`[ ]` = отметка. Раньше каждая
  // строка была своим <input>, из-за чего по списку нельзя было ходить
  // стрелками и выделять несколько строк сразу. Изменения по-прежнему
  // применяются при сохранении (diff с task.subtasks), а не мгновенно.
  //
  // Соответствие «строка ↔ существующая подзадача» ведётся по позиции: id
  // текста не переживает, а альтернатива (скрытые маркеры в тексте) сломала бы
  // именно то, ради чего поле текстовое. Практическое следствие — переставленная
  // строка считается переименованием, а не перемещением; для чек-листа из
  // нескольких пунктов это дешевле, чем сопоставление по содержимому.
  // svelte-ignore state_referenced_locally -- модалка пересоздаётся на каждую задачу ({#if editingTask}), снимок начального значения тут и нужен
  let subsText = $state(
    formatChecklist((task?.subtasks ?? []).map(s => ({ title: s.title, done: s.done }))),
  );
  const hasSubs = $derived(parseChecklist(subsText).length > 0);

  // --- Шаблоны чеклистов (v0.7.15, перенесено в модалку в v0.8.3) ---
  let checklistTemplates: ChecklistTemplate[] = $state([]);
  let templatePickerOpen = $state(false);
  let savingTemplateOpen = $state(false);
  let newTemplateName = $state("");

  async function loadChecklistTemplates() {
    checklistTemplates = await api.getChecklistTemplates().catch(() => []);
  }

  function toggleTemplatePicker() {
    templatePickerOpen = !templatePickerOpen;
    savingTemplateOpen = false;
    if (templatePickerOpen) loadChecklistTemplates();
  }

  function applyTemplate(template: ChecklistTemplate) {
    // Шаблон дописывается к тому, что уже набрано, а не заменяет его.
    const added = formatChecklist(template.items.map(title => ({ title, done: false })));
    subsText = subsText.trim() ? `${subsText.replace(/\n+$/, "")}\n${added}` : added;
    templatePickerOpen = false;
  }

  async function removeTemplate(id: string) {
    await api.deleteChecklistTemplate(id);
    await loadChecklistTemplates();
  }

  function toggleSaveTemplate() {
    savingTemplateOpen = !savingTemplateOpen;
    templatePickerOpen = false;
    newTemplateName = "";
  }

  async function saveCurrentAsTemplate() {
    const name = newTemplateName.trim();
    const items = parseChecklist(subsText).map(s => s.title);
    if (!name || items.length === 0) return;
    await api.createChecklistTemplate(name, items);
    savingTemplateOpen = false;
    newTemplateName = "";
  }

  // Diff чеклиста против исходных подзадач задачи. Строки текста сопоставляются
  // с существующими подзадачами по позиции (id в тексте не хранится, см.
  // комментарий у subsText): i-я строка правит i-ю подзадачу, лишние строки
  // добавляются, лишние подзадачи удаляются. Так правка формулировки остаётся
  // переименованием и сохраняет отметку, а не пересоздаёт подзадачу заново.
  async function applySubtaskChanges(taskId: string) {
    const orig = task?.subtasks ?? [];
    const current = parseChecklist(subsText);
    for (let i = current.length; i < orig.length; i++) {
      await api.deleteSubtask(orig[i].id);
    }
    for (let i = 0; i < current.length; i++) {
      const s = current[i];
      const o = orig[i];
      if (!o) {
        const added = await api.addSubtask(taskId, s.title);
        if (s.done) await api.toggleSubtask(added.id);
      } else {
        if (o.title !== s.title) await api.renameSubtask(o.id, s.title);
        if (o.done !== s.done) await api.toggleSubtask(o.id);
      }
    }
  }

  function buildRecurrence(): Recurrence {
    switch (recurrenceKey) {
      case "Hourly":   return "Hourly";
      case "Daily":    return "Daily";
      case "Weekly":   return "Weekly";
      case "Custom":   return { Custom: [customN, customUnit] };
      case "Weekdays": {
        const mask = weekdaysMask();
        return mask === 0 ? "None" : { Weekdays: mask };
      }
      default:         return "None";
    }
  }

  function parseTags(s: string): string[] {
    return s.split(",").map(t => t.trim()).filter(Boolean);
  }

  async function handleSave() {
    if (!title.trim()) { error = t("Название не может быть пустым"); return; }
    if (recurrenceKey === "Weekdays" && weekdaysMask() === 0) {
      error = t("Выберите хотя бы один день недели");
      return;
    }
    saving = true;
    error = "";
    try {
      const recurrence = buildRecurrence();
      // Дедлайн больше не обнуляется при повторе — это время первого
      // срабатывания, тот же смысл, что и без повтора (см. next_occurrence
      // на бэкенде, которая сдвигает именно это поле при выполнении).
      const deadlineIso = deadline ? new Date(deadline).toISOString() : null;

      if (isEdit) {
        // Подзадачи — до onSave: onSave обновляет задачу и перечитывает store,
        // подхватывая заодно и изменения чеклиста.
        await applySubtaskChanges(task!.id);
        const patch: UpdateTaskPayload = {
          title: title.trim(),
          description: description.trim() || undefined,
          status,
          priority,
          category,
          tags: parseTags(tagsInput),
          recurrence,
          project_id: projectId,
          ...(deadlineIso ? { deadline: deadlineIso } : {}),
        };
        await onSave(patch);
      } else {
        const payload: CreateTaskPayload = {
          title: title.trim(),
          description: description.trim() || null,
          status,
          priority,
          category,
          tags: parseTags(tagsInput),
          recurrence,
          deadline: deadlineIso,
          project_id: projectId || null,
        };
        const created = await onSave(payload);
        const newSubs = parseChecklist(subsText);
        if (created && "id" in created && newSubs.length > 0) {
          for (const s of newSubs) {
            const added = await api.addSubtask(created.id, s.title);
            if (s.done) await api.toggleSubtask(added.id);
          }
          await taskStore.load(); // create уже перечитал store ДО подзадач
        }
      }
      onClose();
    } catch (e) {
      error = typeof e === "string" ? e : t("Ошибка при сохранении");
    } finally {
      saving = false;
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") handleSave();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div role="dialog" aria-modal="true" class="overlay backdrop" onclick={handleBackdropClick}>
  <div class="modal dialog">
    <h2 class="dialog-title">{isEdit ? t("Редактировать задачу") : t("Новая задача")}</h2>

    {#if error}
      <div class="alert" style="margin:0;">{error}</div>
    {/if}

    <label class="field">
      <span class="label">{t("Название *")}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={title} placeholder={t("Название задачи")} autofocus />
    </label>

    <label class="field">
      <span class="label">{t("Описание")}</span>
      <textarea bind:value={description} placeholder={t("Описание (необязательно)")} rows="3" style="resize:vertical;"></textarea>
    </label>

    <div class="field">
      <span class="label">{t("Подзадачи")}</span>
      <ChecklistEditor bind:value={subsText} placeholder={t("Подзадача на строку (Enter — ещё строка)")} />
      <div class="template-row">
        <button type="button" class="btn-sm" onclick={toggleTemplatePicker}>{t("Из шаблона…")}</button>
        <button type="button" class="btn-sm" onclick={toggleSaveTemplate}
          disabled={!hasSubs}
          title={hasSubs ? "" : t("Сначала добавьте подзадачи")}>
          {t("Сохранить как шаблон")}
        </button>
      </div>

      {#if templatePickerOpen}
        <div class="template-panel">
          {#if checklistTemplates.length === 0}
            <span class="muted" style="font-size:12px;">{t("Нет сохранённых шаблонов")}</span>
          {:else}
            {#each checklistTemplates as tpl (tpl.id)}
              <div class="template-line">
                <span style="flex:1;">{tpl.name} <span class="muted">({tpl.items.length})</span></span>
                <button type="button" class="btn-sm" onclick={() => applyTemplate(tpl)}>{t("Применить")}</button>
                <button type="button" class="btn-icon btn-danger" title={t("Удалить шаблон")} onclick={() => removeTemplate(tpl.id)}>✕</button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      {#if savingTemplateOpen}
        <div class="template-panel template-line">
          <input
            type="text"
            placeholder={t("Название шаблона")}
            bind:value={newTemplateName}
            onkeydown={(e) => { if (e.key === 'Enter') saveCurrentAsTemplate(); }}
            class="sub-input"
          />
          <button type="button" class="btn-sm btn-primary" onclick={saveCurrentAsTemplate} disabled={!newTemplateName.trim()}>
            {t("Сохранить")}
          </button>
        </div>
      {/if}
    </div>

    <div class="pair">
      <label class="field">
        <span class="label">{t("Приоритет")}</span>
        <select bind:value={priority}>
          <option value="Low">{t("Низкий")}</option>
          <option value="Medium">{t("Средний")}</option>
          <option value="High">{t("Высокий")}</option>
          <option value="Critical">{t("Критический")}</option>
        </select>
      </label>
      <label class="field">
        <span class="label">{t("Категория")}</span>
        <select bind:value={category}>
          {#each categoryStore.categories as c (c.id)}
            <option value={c.id}>{categoryStore.name(c.id)}</option>
          {/each}
        </select>
      </label>
    </div>

    {#if isEdit && totalTaskMins > 0}
      <div class="field">
        <span class="label">{t("Время всего")}</span>
        <span class="muted" style="font-size:13px;">{t("{n} мин", { n: totalTaskMins })}</span>
      </div>
    {/if}

    {#if isEdit}
      <label class="field">
        <span class="label">{t("Статус")}</span>
        <select bind:value={status}>
          {#each statusStore.statuses as s (s.id)}
            <option value={s.id}>{statusStore.name(s.id)}</option>
          {/each}
        </select>
      </label>
    {/if}

    <div class="field recurrence-block">
      <span class="label">{t("Дедлайн и повтор")}</span>
      <div class="pair">
        <label class="field">
          <span class="sublabel">{recurrenceKey === "None" ? t("Дедлайн") : t("Первое срабатывание")}</span>
          <input type="datetime-local" bind:value={deadline} />
        </label>
        <label class="field">
          <span class="sublabel">{t("Повтор")}</span>
          <select bind:value={recurrenceKey}>
            <option value="None">{t("Без повтора")}</option>
            <option value="Hourly">{t("Каждый час")}</option>
            <option value="Daily">{t("Каждый день")}</option>
            <option value="Weekly">{t("Каждую неделю")}</option>
            <option value="Custom">{t("Свой интервал")}</option>
            <option value="Weekdays">{t("По дням недели")}</option>
          </select>
        </label>
      </div>

      {#if recurrenceKey === "Custom"}
        <div class="custom-row">
          <span>{t("Каждые")}</span>
          <input type="number" bind:value={customN} min="1" style="width:64px;" />
          <select bind:value={customUnit}>
            <option value="Minutes">{t("минут")}</option>
            <option value="Hours">{t("часов")}</option>
            <option value="Days">{t("дней")}</option>
            <option value="Weeks">{t("недель")}</option>
          </select>
        </div>
      {/if}

      {#if recurrenceKey === "Weekdays"}
        <div class="day-picker">
          {#each WEEKDAY_LABELS as d, i}
            <label class="day-chip">
              <input type="checkbox" bind:checked={weekdays[i]} />
              <span>{d}</span>
            </label>
          {/each}
        </div>
      {/if}

      {#if recurrenceKey !== "None"}
        <span class="hint">{t("При выполнении задача не закрывается — дедлайн сам сдвинется на следующий срок, задача останется активной.")}</span>
      {/if}
    </div>

    <label class="field">
      <span class="label">{t("Теги (через запятую)")}</span>
      <input bind:value={tagsInput} placeholder={t("работа, важное, срочное")} />
    </label>

    {#if projectStore.active.length > 0 || projectId}
      <label class="field">
        <span class="label">{t("Проект")}</span>
        <select bind:value={projectId}>
          <option value="">{t("Без проекта")}</option>
          {#each projectStore.active as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
          <!-- задача может висеть на архивном проекте — не теряем привязку -->
          {#each projectStore.projects.filter(p => p.archived && p.id === projectId) as p (p.id)}
            <option value={p.id}>{p.name} ({t("архив")})</option>
          {/each}
        </select>
      </label>
    {/if}

    <!-- Зависимости только у сохранённой задачи (v0.9.56): связь пишется в
         отдельную таблицу по id, которого у новой задачи ещё нет. Правки тут
         применяются сразу, не по кнопке «Сохранить» — как и подзадачи. -->
    {#if isEdit && task}
      <div class="field">
        <span class="label">{t("Блокируется задачами")}</span>
        {#if blockedBy.length > 0}
          <ul class="blockers">
            {#each blockedBy as b (b.id)}
              <li>
                <span class="blocker-title">{b.title}</span>
                <button
                  class="btn-ghost blocker-del"
                  onclick={() => removeBlocker(b.id)}
                  title={t("Убрать зависимость")}
                  aria-label={t("Убрать зависимость")}
                >×</button>
              </li>
            {/each}
          </ul>
        {/if}
        <select value="" onchange={(e) => addBlocker(e.currentTarget)}>
          <option value="">{t("Добавить блокер...")}</option>
          {#each candidateBlockers as c (c.id)}
            <option value={c.id}>{c.title}</option>
          {/each}
        </select>
      </div>
    {/if}

    <div class="actions">
      <span class="muted" style="font-size:11px;margin-right:auto;"><kbd>Ctrl Enter</kbd> {t("сохранить ·")} <kbd>Esc</kbd> {t("закрыть")}</span>
      <button class="btn-ghost" onclick={onClose}>{t("Отмена")}</button>
      <button class="btn-primary" onclick={handleSave} disabled={saving || !title.trim()}>
        {saving ? t("Сохранение...") : isEdit ? t("Сохранить") : t("Создать")}
      </button>
    </div>
  </div>
</div>

<style>
  /* Зависимости (v0.9.56) */
  .blockers {
    list-style: none;
    margin: 0 0 6px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .blockers li {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-hover);
    border-radius: var(--radius);
    padding: 4px 6px 4px 9px;
    font-size: 12px;
  }

  .blocker-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .blocker-del {
    padding: 0 6px;
    line-height: 1;
    font-size: 15px;
  }

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

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  /* Дедлайн+Повтор сгруппированы визуально (v0.9.21) — общая рамка
     объясняет, что это одна связанная настройка, а не два независимых поля. */
  .recurrence-block {
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    gap: 8px;
  }

  .sublabel {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .hint {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .custom-row {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
  }

  .day-picker {
    display: flex;
    gap: 4px;
  }

  .day-chip {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 12px;
  }

  .day-chip input {
    margin: 0;
  }

  /* Чек-лист подзадач переехал в ChecklistEditor (v0.9.45) — стили строк
     живут там же, здесь остаётся только ряд кнопок шаблонов. */
  .template-row {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }

  .template-panel {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    margin-top: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
  }

  .template-line {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .sub-input {
    flex: 1;
    font-size: 12px;
    padding: 2px 8px;
  }

  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    margin-top: 4px;
  }
</style>
