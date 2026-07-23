<script lang="ts">
  import { tick } from "svelte";
  import type { Task, CreateTaskPayload, UpdateTaskPayload, Priority, Category, Recurrence, RecurrenceUnit, TaskStatus, ChecklistTemplate } from "../types";
  import { api } from "../api/tauri";
  import { projectStore } from "../stores/projects.svelte";
  import { categoryStore } from "../stores/categories.svelte";
  import { statusStore } from "../stores/statuses.svelte";
  import { taskStore } from "../stores/tasks.svelte";

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
  const WEEKDAY_LABELS = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
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

  // --- Инлайн-чеклист подзадач (v0.8.3, стиль Xiaomi Notes): каждая строка —
  // чекбокс + текст; Enter — новая строка ниже, Backspace на пустой — удалить
  // строку. Изменения применяются при сохранении (diff с task.subtasks).
  type SubDraft = { id: string | null; title: string; done: boolean };
  // svelte-ignore state_referenced_locally -- модалка пересоздаётся на каждую задачу ({#if editingTask}), снимок начального значения тут и нужен
  let subs = $state<SubDraft[]>([
    ...(task?.subtasks ?? []).map(s => ({ id: s.id, title: s.title, done: s.done })),
    { id: null, title: "", done: false },
  ]);
  let subEls: HTMLInputElement[] = $state([]);

  async function onSubKeydown(e: KeyboardEvent, i: number) {
    if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      subs.splice(i + 1, 0, { id: null, title: "", done: false });
      await tick();
      subEls[i + 1]?.focus();
    } else if (e.key === "Backspace" && subs[i].title === "" && subs.length > 1) {
      e.preventDefault();
      subs.splice(i, 1);
      await tick();
      subEls[Math.max(0, i - 1)]?.focus();
    }
  }

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

  async function applyTemplate(template: ChecklistTemplate) {
    // Последняя строка драфта — пустая заготовка; вставляем шаблон перед ней.
    const blankIdx = subs.length - 1;
    const items = template.items.map(title => ({ id: null, title, done: false }));
    subs.splice(blankIdx, 0, ...items);
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
    const items = subs.filter(s => s.title.trim()).map(s => s.title.trim());
    if (!name || items.length === 0) return;
    await api.createChecklistTemplate(name, items);
    savingTemplateOpen = false;
    newTemplateName = "";
  }

  // Diff чеклиста против исходных подзадач задачи: удаление пропавших,
  // rename/toggle изменившихся, добавление новых (пустые строки игнорируются).
  async function applySubtaskChanges(taskId: string) {
    const orig = task?.subtasks ?? [];
    const current = subs.filter(s => s.title.trim());
    const keptIds = new Set(current.map(s => s.id).filter(Boolean));
    for (const o of orig) {
      if (!keptIds.has(o.id)) await api.deleteSubtask(o.id);
    }
    for (const s of current) {
      if (s.id === null) {
        const added = await api.addSubtask(taskId, s.title.trim());
        if (s.done) await api.toggleSubtask(added.id);
      } else {
        const o = orig.find(x => x.id === s.id);
        if (!o) continue;
        if (o.title !== s.title.trim()) await api.renameSubtask(s.id, s.title.trim());
        if (o.done !== s.done) await api.toggleSubtask(s.id);
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
    if (!title.trim()) { error = "Название не может быть пустым"; return; }
    saving = true;
    error = "";
    try {
      const recurrence = buildRecurrence();
      const deadlineIso = recurrenceKey === "None" && deadline
        ? new Date(deadline).toISOString()
        : null;

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
        const newSubs = subs.filter(s => s.title.trim());
        if (created && "id" in created && newSubs.length > 0) {
          for (const s of newSubs) {
            const added = await api.addSubtask(created.id, s.title.trim());
            if (s.done) await api.toggleSubtask(added.id);
          }
          await taskStore.load(); // create уже перечитал store ДО подзадач
        }
      }
      onClose();
    } catch (e) {
      error = typeof e === "string" ? e : "Ошибка при сохранении";
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
    <h2 class="dialog-title">{isEdit ? "Редактировать задачу" : "Новая задача"}</h2>

    {#if error}
      <div class="alert" style="margin:0;">{error}</div>
    {/if}

    <label class="field">
      <span class="label">Название *</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={title} placeholder="Название задачи" autofocus />
    </label>

    <label class="field">
      <span class="label">Описание</span>
      <textarea bind:value={description} placeholder="Описание (необязательно)" rows="3" style="resize:vertical;"></textarea>
    </label>

    <div class="field">
      <span class="label">Подзадачи</span>
      <div class="checklist">
        {#each subs as s, i}
          <div class="check-row">
            <input type="checkbox" bind:checked={s.done} tabindex="-1" />
            <input
              class="check-input"
              bind:value={s.title}
              bind:this={subEls[i]}
              placeholder={i === subs.length - 1 && !s.title ? "+ подзадача (Enter — ещё строка)" : ""}
              onkeydown={(e) => onSubKeydown(e, i)}
            />
          </div>
        {/each}
      </div>
      <div class="template-row">
        <button type="button" class="btn-sm" onclick={toggleTemplatePicker}>Из шаблона…</button>
        <button type="button" class="btn-sm" onclick={toggleSaveTemplate}
          disabled={!subs.some(s => s.title.trim())}
          title={subs.some(s => s.title.trim()) ? "" : "Сначала добавьте подзадачи"}>
          Сохранить как шаблон
        </button>
      </div>

      {#if templatePickerOpen}
        <div class="template-panel">
          {#if checklistTemplates.length === 0}
            <span class="muted" style="font-size:12px;">Нет сохранённых шаблонов</span>
          {:else}
            {#each checklistTemplates as tpl (tpl.id)}
              <div class="template-line">
                <span style="flex:1;">{tpl.name} <span class="muted">({tpl.items.length})</span></span>
                <button type="button" class="btn-sm" onclick={() => applyTemplate(tpl)}>Применить</button>
                <button type="button" class="btn-icon btn-danger" title="Удалить шаблон" onclick={() => removeTemplate(tpl.id)}>✕</button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      {#if savingTemplateOpen}
        <div class="template-panel template-line">
          <input
            type="text"
            placeholder="Название шаблона"
            bind:value={newTemplateName}
            onkeydown={(e) => { if (e.key === 'Enter') saveCurrentAsTemplate(); }}
            class="sub-input"
          />
          <button type="button" class="btn-sm btn-primary" onclick={saveCurrentAsTemplate} disabled={!newTemplateName.trim()}>
            Сохранить
          </button>
        </div>
      {/if}
    </div>

    <div class="pair">
      <label class="field">
        <span class="label">Приоритет</span>
        <select bind:value={priority}>
          <option value="Low">Низкий</option>
          <option value="Medium">Средний</option>
          <option value="High">Высокий</option>
          <option value="Critical">Критический</option>
        </select>
      </label>
      <label class="field">
        <span class="label">Категория</span>
        <select bind:value={category}>
          {#each categoryStore.categories as c (c.id)}
            <option value={c.id}>{c.name}</option>
          {/each}
        </select>
      </label>
    </div>

    {#if isEdit && totalTaskMins > 0}
      <div class="field">
        <span class="label">Время всего</span>
        <span class="muted" style="font-size:13px;">{totalTaskMins} мин</span>
      </div>
    {/if}

    {#if isEdit}
      <label class="field">
        <span class="label">Статус</span>
        <select bind:value={status}>
          {#each statusStore.statuses as s (s.id)}
            <option value={s.id}>{s.name}</option>
          {/each}
        </select>
      </label>
    {/if}

    <div class="pair">
      <label class="field">
        <span class="label">Дедлайн</span>
        <input type="datetime-local" bind:value={deadline} disabled={recurrenceKey !== "None"} />
      </label>
      <label class="field">
        <span class="label">Повтор</span>
        <select bind:value={recurrenceKey}>
          <option value="None">Без повтора</option>
          <option value="Hourly">Каждый час</option>
          <option value="Daily">Каждый день</option>
          <option value="Weekly">Каждую неделю</option>
          <option value="Custom">Свой интервал</option>
          <option value="Weekdays">По дням недели</option>
        </select>
      </label>
    </div>

    {#if recurrenceKey === "Custom"}
      <div class="custom-row">
        <span>Каждые</span>
        <input type="number" bind:value={customN} min="1" style="width:64px;" />
        <select bind:value={customUnit}>
          <option value="Minutes">минут</option>
          <option value="Hours">часов</option>
          <option value="Days">дней</option>
          <option value="Weeks">недель</option>
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

    <label class="field">
      <span class="label">Теги (через запятую)</span>
      <input bind:value={tagsInput} placeholder="работа, важное, срочное" />
    </label>

    {#if projectStore.active.length > 0 || projectId}
      <label class="field">
        <span class="label">Проект</span>
        <select bind:value={projectId}>
          <option value="">Без проекта</option>
          {#each projectStore.active as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
          <!-- задача может висеть на архивном проекте — не теряем привязку -->
          {#each projectStore.projects.filter(p => p.archived && p.id === projectId) as p (p.id)}
            <option value={p.id}>{p.name} (архив)</option>
          {/each}
        </select>
      </label>
    {/if}

    <div class="actions">
      <span class="muted" style="font-size:11px;margin-right:auto;"><kbd>Ctrl Enter</kbd> сохранить · <kbd>Esc</kbd> закрыть</span>
      <button class="btn-ghost" onclick={onClose}>Отмена</button>
      <button class="btn-primary" onclick={handleSave} disabled={saving || !title.trim()}>
        {saving ? "Сохранение..." : isEdit ? "Сохранить" : "Создать"}
      </button>
    </div>
  </div>
</div>

<style>
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

  /* Инлайн-чеклист подзадач (v0.8.3): строки без рамок, как в блокнотах */
  .checklist {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .check-row input[type="checkbox"] {
    flex-shrink: 0;
  }

  .check-input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 3px 4px;
    font-size: 13px;
    border-bottom: 1px solid transparent;
    border-radius: 0;
  }
  .check-input:focus {
    outline: none;
    border-bottom-color: var(--accent);
  }

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
