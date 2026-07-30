<script lang="ts">
  // Открытие задачи по id прямо на текущем экране (v0.9.53).
  //
  // Раньше клик по задаче в Календаре, на Дашборде и в «Сегодня» уводил на
  // экран Задач: `activeView = "tasks"` + `taskStore.requestFocus(id)`.
  // Пользователь терял контекст — смотрел неделю, кликнул задачу и оказывался
  // в другом разделе, откуда надо возвращаться вручную.
  //
  // Логика открытия не дублируется по экранам, а живёт здесь: она не сводится
  // к «показать модалку». Завершённая задача (hidden) — это история, её надо
  // открывать read-only, иначе клик по выполненной задаче из попапа дня
  // предлагал бы править дедлайн и повтор у того, что давно сделано. Три
  // копии этого правила неизбежно разъехались бы.
  import TaskModal from "./TaskModal.svelte";
  import TaskHistoryDetail from "./TaskHistoryDetail.svelte";
  import { taskStore } from "../stores/tasks.svelte";
  import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

  type Props = {
    // Какую задачу открыть; null — ничего не открыто.
    taskId: string | null;
    onClose: () => void;
  };

  let { taskId, onClose }: Props = $props();

  // Экраны, открывающие задачу, не всегда грузили список (Календарь берёт
  // задачи своим запросом) — без этого модалка не нашла бы задачу по id.
  if (taskStore.tasks.length === 0) taskStore.load();

  const task = $derived<Task | null>(
    taskId ? taskStore.tasks.find(t => t.id === taskId) ?? null : null,
  );

  async function handleEdit(data: CreateTaskPayload | UpdateTaskPayload) {
    if (!task) return;
    await taskStore.update(task.id, data as UpdateTaskPayload);
  }
</script>

{#if task}
  {#if task.hidden}
    <TaskHistoryDetail task={task} onClose={onClose} />
  {:else}
    <TaskModal task={task} onSave={handleEdit} onClose={onClose} />
  {/if}
{/if}
