<script lang="ts">
  import { onMount } from "svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import type { Task, CreateTaskPayload } from "../lib/types";

  let { onOpenTask }: { onOpenTask: (id: string) => void } = $props();

  const today = new Date();
  let year = $state(today.getFullYear());
  let month = $state(today.getMonth()); // 0-11

  onMount(() => {
    taskStore.load();
  });

  const MONTHS = [
    "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
    "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь",
  ];
  const WEEKDAYS = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

  function localDateKey(d: Date): string {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  }

  // Задачи по локальной дате дедлайна (скрытые из истории не показываем).
  const tasksByDay = $derived.by(() => {
    const map = new Map<string, Task[]>();
    for (const t of taskStore.activeTasks) {
      if (!t.deadline) continue;
      const key = localDateKey(new Date(t.deadline));
      const list = map.get(key) ?? [];
      list.push(t);
      map.set(key, list);
    }
    return map;
  });

  interface DayCell {
    key: string;
    day: number;
    inMonth: boolean;
    isToday: boolean;
    tasks: Task[];
  }

  // Сетка месяца: недели с понедельника, всегда полные строки по 7.
  const grid = $derived.by(() => {
    const first = new Date(year, month, 1);
    const lead = (first.getDay() + 6) % 7; // сколько дней прошлого месяца показать
    const start = new Date(year, month, 1 - lead);
    const todayKey = localDateKey(new Date());

    const cells: DayCell[] = [];
    const d = new Date(start);
    do {
      const key = localDateKey(d);
      cells.push({
        key,
        day: d.getDate(),
        inMonth: d.getMonth() === month,
        isToday: key === todayKey,
        tasks: tasksByDay.get(key) ?? [],
      });
      d.setDate(d.getDate() + 1);
    } while (d.getMonth() === month || cells.length % 7 !== 0);
    return cells;
  });

  function shiftMonth(delta: number) {
    const d = new Date(year, month + delta, 1);
    year = d.getFullYear();
    month = d.getMonth();
  }

  function goToday() {
    year = today.getFullYear();
    month = today.getMonth();
  }

  function chipClass(t: Task): string {
    if (t.status === "Done" || t.status === "Archived") return "done";
    if (t.deadline && new Date(t.deadline) < new Date()) return "overdue";
    return "";
  }

  const MAX_CHIPS = 3;

  // Клик по дню — создание задачи с дедлайном на этот день (ключ ячейки).
  let createFor = $state<string | null>(null);

  async function handleCreate(data: unknown) {
    await taskStore.create(data as CreateTaskPayload);
  }
</script>

<div class="cal">
  <div class="page-head">
    <h2 class="page-title">Календарь</h2>
    <span style="flex:1;"></span>
    <button class="btn-icon" onclick={() => shiftMonth(-1)} title="Предыдущий месяц">←</button>
    <span class="month-label">{MONTHS[month]} {year}</span>
    <button class="btn-icon" onclick={() => shiftMonth(1)} title="Следующий месяц">→</button>
    <button class="btn-sm" onclick={goToday}>Сегодня</button>
  </div>

  <div class="month-grid">
    {#each WEEKDAYS as wd}
      <div class="weekday">{wd}</div>
    {/each}

    {#each grid as cell (cell.key)}
      <div
        class="day card"
        class:today={cell.isToday}
        class:out={!cell.inMonth}
        onclick={() => createFor = cell.key}
        onkeydown={(e) => { if (e.key === "Enter" && e.target === e.currentTarget) createFor = cell.key; }}
        role="button"
        tabindex="0"
        title="Создать задачу на этот день"
      >
        <div class="day-num" class:today={cell.isToday}>{cell.day}</div>
        <div class="day-tasks">
          {#each cell.tasks.slice(0, MAX_CHIPS) as t (t.id)}
            <button class="task-chip {chipClass(t)}" onclick={(e) => { e.stopPropagation(); onOpenTask(t.id); }} title={t.title}>
              {t.title}
            </button>
          {/each}
          {#if cell.tasks.length > MAX_CHIPS}
            <span class="more">+{cell.tasks.length - MAX_CHIPS} ещё</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <p class="muted" style="font-size:12px;margin-top:10px;">
    Задачи разложены по дате дедлайна. Красные — просроченные, зачёркнутые — выполненные.
    Клик по задаче открывает её, клик по дню — создаёт задачу с дедлайном на этот день.
  </p>
</div>

{#if createFor}
  <TaskModal
    initialDeadline={`${createFor}T09:00`}
    onSave={handleCreate}
    onClose={() => createFor = null}
  />
{/if}

<style>
  .page-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 12px;
  }

  .month-label {
    min-width: 130px;
    text-align: center;
    font-weight: 600;
    font-size: 13px;
  }

  .month-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 4px;
  }

  .weekday {
    text-align: center;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .04em;
    color: var(--text-secondary);
    padding: 2px 0 4px;
  }

  .day {
    min-height: 86px;
    padding: 4px;
    min-width: 0;
    cursor: pointer;
  }

  .day:hover {
    background: var(--bg-hover);
  }

  .day.today {
    border-color: var(--accent);
  }

  .day.out {
    opacity: 0.45;
  }

  .day-num {
    font-size: 11px;
    color: var(--text-secondary);
    margin-bottom: 3px;
    padding-left: 2px;
  }

  .day-num.today {
    color: var(--accent);
    font-weight: 700;
  }

  .day-tasks {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .task-chip {
    display: block;
    width: 100%;
    text-align: left;
    font-size: 11px;
    padding: 2px 5px;
    border-radius: 4px;
    border: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }

  .task-chip:hover {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }

  .task-chip.overdue {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }

  .task-chip.overdue:hover {
    background: color-mix(in srgb, var(--danger) 24%, transparent);
  }

  .task-chip.done {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    text-decoration: line-through;
    padding: 1px 4px;
  }

  .more {
    font-size: 11px;
    color: var(--text-secondary);
    padding-left: 5px;
  }
</style>
