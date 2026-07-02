<script lang="ts">
  import type { Task, CreateTaskPayload, UpdateTaskPayload, Priority, Category, Recurrence, RecurrenceUnit, TaskStatus } from "../types";

  type Props = {
    task?: Task | null;
    onSave: (data: CreateTaskPayload | UpdateTaskPayload) => Promise<void>;
    onClose: () => void;
  };

  let { task = null, onSave, onClose }: Props = $props();

  const isEdit = !!task;

  let title = $state(task?.title ?? "");
  let description = $state(task?.description ?? "");
  let status = $state<TaskStatus>(task?.status ?? "Todo");
  let priority = $state<Priority>(task?.priority ?? "Medium");
  let category = $state<Category>(task?.category ?? "Work");
  let tagsInput = $state((task?.tags ?? []).join(", "));
  let deadline = $state(task?.deadline ? new Date(task.deadline).toISOString().slice(0, 16) : "");

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

<!-- Backdrop -->
<div
  role="dialog"
  aria-modal="true"
  onclick={handleBackdropClick}
  style="
    position:fixed;inset:0;z-index:100;
    background:rgba(0,0,0,0.5);
    display:flex;align-items:center;justify-content:center;
    padding:16px;
  "
>
  <div style="
    background:var(--bg, #fff);
    border-radius:10px;
    padding:24px;
    width:100%;max-width:520px;
    max-height:90vh;overflow-y:auto;
    box-shadow:0 20px 60px rgba(0,0,0,0.3);
    display:flex;flex-direction:column;gap:14px;
  ">
    <h2 style="margin:0;font-size:18px;">{isEdit ? "Редактировать задачу" : "Новая задача"}</h2>

    {#if error}
      <div style="background:#fee2e2;color:#dc2626;padding:8px 10px;border-radius:6px;font-size:13px;">{error}</div>
    {/if}

    <!-- Title -->
    <div style="display:flex;flex-direction:column;gap:4px;">
      <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">НАЗВАНИЕ *</span>
      <input
        bind:value={title}
        placeholder="Название задачи"
        autofocus
        style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;"
      />
    </div>

    <!-- Description -->
    <div style="display:flex;flex-direction:column;gap:4px;">
      <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">ОПИСАНИЕ</span>
      <textarea
        bind:value={description}
        placeholder="Описание (необязательно)"
        rows="3"
        style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;resize:vertical;"
      ></textarea>
    </div>

    <!-- Priority + Category -->
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px;">
      <div style="display:flex;flex-direction:column;gap:4px;">
        <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">ПРИОРИТЕТ</span>
        <select bind:value={priority} style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;">
          <option value="Low">Низкий</option>
          <option value="Medium">Средний</option>
          <option value="High">Высокий</option>
          <option value="Critical">Критический</option>
        </select>
      </div>
      <div style="display:flex;flex-direction:column;gap:4px;">
        <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">КАТЕГОРИЯ</span>
        <select bind:value={category} style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;">
          <option value="Work">Работа</option>
          <option value="Study">Учёба</option>
          <option value="Home">Дом</option>
          <option value="Health">Здоровье</option>
          <option value="Other">Другое</option>
        </select>
      </div>
    </div>

    <!-- Status (edit only) -->
    {#if isEdit}
      <div style="display:flex;flex-direction:column;gap:4px;">
        <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">СТАТУС</span>
        <select bind:value={status} style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;">
          <option value="Todo">К выполнению</option>
          <option value="InProgress">В процессе</option>
          <option value="Done">Выполнено</option>
          <option value="Archived">Архив</option>
        </select>
      </div>
    {/if}

    <!-- Deadline -->
    <div style="display:flex;flex-direction:column;gap:4px;">
      <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">ДЕДЛАЙН</span>
      <input
        type="datetime-local"
        bind:value={deadline}
        disabled={recurrenceKey !== "None"}
        style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;"
      />
    </div>

    <!-- Recurrence -->
    <div style="display:flex;flex-direction:column;gap:6px;">
      <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">ПОВТОР</span>
      <select bind:value={recurrenceKey} style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;">
        <option value="None">Без повтора</option>
        <option value="Hourly">Каждый час</option>
        <option value="Daily">Каждый день</option>
        <option value="Weekly">Каждую неделю</option>
        <option value="Custom">Свой интервал</option>
      </select>
      {#if recurrenceKey === "Custom"}
        <div style="display:flex;gap:8px;align-items:center;">
          <span style="font-size:13px;">Каждые</span>
          <input type="number" bind:value={customN} min="1" style="width:60px;padding:6px 8px;border:1px solid var(--border,#e5e7eb);border-radius:6px;" />
          <select bind:value={customUnit} style="padding:6px 8px;border:1px solid var(--border,#e5e7eb);border-radius:6px;">
            <option value="Minutes">минут</option>
            <option value="Hours">часов</option>
            <option value="Days">дней</option>
            <option value="Weeks">недель</option>
          </select>
        </div>
      {/if}
    </div>

    <!-- Tags -->
    <div style="display:flex;flex-direction:column;gap:4px;">
      <span style="font-size:12px;font-weight:600;color:var(--text-secondary,#6b7280);">ТЕГИ (через запятую)</span>
      <input
        bind:value={tagsInput}
        placeholder="работа, важное, срочное"
        style="padding:8px 10px;border:1px solid var(--border,#e5e7eb);border-radius:6px;font-size:14px;"
      />
    </div>

    <!-- Actions -->
    <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:4px;">
      <button
        onclick={onClose}
        style="padding:8px 16px;border:1px solid var(--border,#e5e7eb);border-radius:6px;background:transparent;cursor:pointer;"
      >Отмена</button>
      <button
        onclick={handleSave}
        disabled={saving || !title.trim()}
        style="padding:8px 16px;border:none;border-radius:6px;background:#2563eb;color:white;cursor:pointer;font-weight:500;"
      >{saving ? "Сохранение..." : isEdit ? "Сохранить" : "Создать"}</button>
    </div>

    <p style="margin:0;font-size:11px;color:var(--text-secondary,#9ca3af);text-align:right;">Ctrl+Enter — сохранить · Esc — закрыть</p>
  </div>
</div>
