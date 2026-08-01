<script lang="ts">
  // Opening a task by id right on the current screen.
  //
  // Clicking a task in the Calendar, on the Dashboard or in "Today" used to lead
  // away to the Tasks screen: `activeView = "tasks"` plus `taskStore.requestFocus(id)`.
  // The user lost their context — looking at the week, clicking a task and ending up
  // in another section they then had to navigate back from.
  //
  // The opening logic is not duplicated across screens but lives here, because it is
  // more than "show a modal". A completed task (hidden) is history and must open
  // read-only, or clicking a completed task in the day popup would offer to edit the
  // deadline and recurrence of something long done. Three copies of that rule would
  // inevitably drift apart.
  import TaskModal from "./TaskModal.svelte";
  import TaskHistoryDetail from "./TaskHistoryDetail.svelte";
  import { taskStore } from "../stores/tasks.svelte";
  import type { Task, CreateTaskPayload, UpdateTaskPayload } from "../types";

  type Props = {
    // Which task to open; null means nothing is open.
    taskId: string | null;
    onClose: () => void;
  };

  let { taskId, onClose }: Props = $props();

  // Screens that open a task did not always load the list (the Calendar fetches tasks
  // with its own query) — without this the modal would not find the task by id.
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
