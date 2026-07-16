<script lang="ts">
  import type { Task, CreateTaskPayload, UpdateTaskPayload, Priority, Category, Recurrence, RecurrenceUnit, TaskStatus } from "../types";
  import { projectStore } from "../stores/projects.svelte";
  import { categoryStore } from "../stores/categories.svelte";

  type Props = {
    task?: Task | null;
    initialDeadline?: string | null; // префилл дедлайна при создании (формат datetime-local)
    onSave: (data: CreateTaskPayload | UpdateTaskPayload) => Promise<void>;
    onClose: () => void;
  };

  let { task = null, initialDeadline = null, onSave, onClose }: Props = $props();

  const isEdit = !!task;

  // Модалку открывают и разделы, не грузившие категории (Календарь)
  if (categoryStore.categories.length === 0) categoryStore.load();

  let title = $state(task?.title ?? "");
  let description = $state(task?.description ?? "");
  let status = $state<TaskStatus>(task?.status ?? "Todo");
  let priority = $state<Priority>(task?.priority ?? "Medium");
  // "Other" — фолбэк-категория, существует всегда (в отличие от Work — её можно удалить)
  let category = $state<Category>(task?.category ?? "Other");
  let tagsInput = $state((task?.tags ?? []).join(", "));
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

  type RecurrenceKey = "None" | "Hourly" | "Daily" | "Weekly" | "Custom";

  function initRecurrenceKey(): RecurrenceKey {
    const r = task?.recurrence;
    if (!r || r === "None") return "None";
    if (r === "Hourly") return "Hourly";
    if (r === "Daily") return "Daily";
    if (r === "Weekly") return "Weekly";
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

  let saving = $state(false);
  let error = $state("");

  function buildRecurrence(): Recurrence {
    switch (recurrenceKey) {
      case "Hourly": return "Hourly";
      case "Daily":  return "Daily";
      case "Weekly": return "Weekly";
      case "Custom": return { Custom: [customN, customUnit] };
      default:       return "None";
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
        await onSave(payload);
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

    {#if isEdit}
      <label class="field">
        <span class="label">Статус</span>
        <select bind:value={status}>
          <option value="Todo">К выполнению</option>
          <option value="InProgress">В процессе</option>
          <option value="Done">Выполнено</option>
          <option value="Archived">Архив</option>
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

  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    margin-top: 4px;
  }
</style>
