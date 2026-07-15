<script lang="ts">
  import { onMount } from "svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import TaskModal from "../lib/components/TaskModal.svelte";
  import type { Task, CreateTaskPayload } from "../lib/types";

  let { onOpenTask }: { onOpenTask: (id: string) => void } = $props();

  const today = new Date();
  let year = $state(today.getFullYear());
  let month = $state(today.getMonth()); // 0-11
  let viewMode = $state<"month" | "week">("month");

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
    if (viewMode === "week") {
      weekAnchor = new Date(weekAnchor.getFullYear(), weekAnchor.getMonth(), weekAnchor.getDate() + delta * 7);
      return;
    }
    const d = new Date(year, month + delta, 1);
    year = d.getFullYear();
    month = d.getMonth();
  }

  function goToday() {
    year = today.getFullYear();
    month = today.getMonth();
    weekAnchor = new Date();
  }

  // ===== Неделя: тайм-блокинг =====
  const HOUR_H = 44; // px на час
  const SNAP_MIN = 15;

  let weekAnchor = $state(new Date());

  function mondayOf(d: Date): Date {
    const day = (d.getDay() + 6) % 7;
    return new Date(d.getFullYear(), d.getMonth(), d.getDate() - day);
  }

  const weekDays = $derived.by(() => {
    const start = mondayOf(weekAnchor);
    const todayKey = localDateKey(new Date());
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
      const key = localDateKey(d);
      return { key, date: d, label: `${WEEKDAYS[i]} ${d.getDate()}`, isToday: key === todayKey };
    });
  });

  const weekLabel = $derived.by(() => {
    const start = mondayOf(weekAnchor);
    const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + 6);
    return `${start.getDate()} ${MONTHS[start.getMonth()].slice(0, 3).toLowerCase()} — ${end.getDate()} ${MONTHS[end.getMonth()].slice(0, 3).toLowerCase()} ${end.getFullYear()}`;
  });

  // Активная переработка размера блока: живой предпросмотр без сохранения на каждый пиксель
  let resizing: { id: string; mins: number } | null = $state(null);

  function blockMins(t: Task): number {
    if (resizing && resizing.id === t.id) return resizing.mins;
    return t.scheduled_mins ?? 60;
  }

  // Блоки по дням недели (только не скрытые задачи)
  const blocksByDay = $derived.by(() => {
    const map = new Map<string, Task[]>();
    for (const t of taskStore.activeTasks) {
      if (!t.scheduled_at) continue;
      const key = localDateKey(new Date(t.scheduled_at));
      const list = map.get(key) ?? [];
      list.push(t);
      map.set(key, list);
    }
    return map;
  });

  // Бэклог: активные задачи без блока (Todo/InProgress)
  const backlog = $derived(
    taskStore.activeTasks.filter(t => !t.scheduled_at && (t.status === "Todo" || t.status === "InProgress"))
  );

  function blockTop(t: Task): number {
    const d = new Date(t.scheduled_at!);
    return ((d.getHours() * 60 + d.getMinutes()) / 60) * HOUR_H;
  }

  function blockLabel(t: Task): string {
    const start = new Date(t.scheduled_at!);
    const end = new Date(start.getTime() + blockMins(t) * 60_000);
    const fmt = (d: Date) => `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    return `${fmt(start)}–${fmt(end)}`;
  }

  function snap(mins: number): number {
    return Math.round(mins / SNAP_MIN) * SNAP_MIN;
  }

  // --- Drag&drop (HTML5): бэклог → слот, блок → другой слот ---
  // dataTransfer хранит только id; смещение хвата держим в модульной переменной.
  let dragOffsetY = 0;

  function onBlockDragStart(e: DragEvent, t: Task) {
    e.dataTransfer?.setData("text/plain", t.id);
    dragOffsetY = e.offsetY;
  }

  function onBacklogDragStart(e: DragEvent, t: Task) {
    e.dataTransfer?.setData("text/plain", t.id);
    dragOffsetY = 0;
  }

  async function onDayDrop(e: DragEvent, dayKey: string) {
    e.preventDefault();
    const id = e.dataTransfer?.getData("text/plain");
    if (!id) return;
    const task = taskStore.tasks.find(t => t.id === id);
    if (!task) return;

    const col = e.currentTarget as HTMLElement;
    const y = e.clientY - col.getBoundingClientRect().top - dragOffsetY;
    const mins = Math.max(0, Math.min(24 * 60 - SNAP_MIN, snap((y / HOUR_H) * 60)));
    const [yy, mm, dd] = dayKey.split("-").map(Number);
    const start = new Date(yy, mm - 1, dd, Math.floor(mins / 60), mins % 60);

    await taskStore.update(id, {
      scheduled_at: start.toISOString(),
      scheduled_mins: task.scheduled_mins ?? 60,
    });
  }

  async function unschedule(id: string) {
    await taskStore.update(id, { scheduled_at: "" });
  }

  // --- Ресайз за нижнюю кромку ---
  function startResize(e: MouseEvent, t: Task) {
    e.preventDefault();
    e.stopPropagation();
    const startY = e.clientY;
    const startMins = t.scheduled_mins ?? 60;
    resizing = { id: t.id, mins: startMins };

    const move = (ev: MouseEvent) => {
      const delta = ((ev.clientY - startY) / HOUR_H) * 60;
      resizing = { id: t.id, mins: Math.max(SNAP_MIN, snap(startMins + delta)) };
    };
    const up = async () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      const mins = resizing?.mins ?? startMins;
      resizing = null;
      if (mins !== startMins) await taskStore.update(t.id, { scheduled_mins: mins });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  // При входе в недельный режим прокручиваем сетку к 8 утра
  let weekScrollEl: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (viewMode === "week" && weekScrollEl) {
      weekScrollEl.scrollTop = 8 * HOUR_H;
    }
  });

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
    <div class="mode-toggle">
      <button class:active-toggle={viewMode === "month"} onclick={() => viewMode = "month"}>Месяц</button>
      <button class:active-toggle={viewMode === "week"} onclick={() => viewMode = "week"}>Неделя</button>
    </div>
    <span style="flex:1;"></span>
    <button class="btn-icon" onclick={() => shiftMonth(-1)} title={viewMode === "week" ? "Предыдущая неделя" : "Предыдущий месяц"}>←</button>
    <span class="month-label">{viewMode === "week" ? weekLabel : `${MONTHS[month]} ${year}`}</span>
    <button class="btn-icon" onclick={() => shiftMonth(1)} title={viewMode === "week" ? "Следующая неделя" : "Следующий месяц"}>→</button>
    <button class="btn-sm" onclick={goToday}>Сегодня</button>
  </div>

  {#if viewMode === "week"}
  <div class="week-layout">
    <div class="week-main card">
      <div class="week-head">
        <div class="hour-gutter-head"></div>
        {#each weekDays as d (d.key)}
          <div class="week-day-head" class:today={d.isToday}>{d.label}</div>
        {/each}
      </div>

      <div class="week-scroll" bind:this={weekScrollEl}>
        <div class="week-grid" style="height:{24 * HOUR_H}px;">
          <div class="hour-gutter">
            {#each Array(24) as _, h}
              <div class="hour-mark" style="height:{HOUR_H}px;">{String(h).padStart(2, "0")}:00</div>
            {/each}
          </div>

          {#each weekDays as d (d.key)}
            <div
              class="week-col"
              class:today={d.isToday}
              role="list"
              ondragover={(e) => e.preventDefault()}
              ondrop={(e) => onDayDrop(e, d.key)}
              style="background-size: 100% {HOUR_H}px;"
            >
              {#each blocksByDay.get(d.key) ?? [] as t (t.id)}
                <div
                  class="block"
                  role="listitem"
                  draggable="true"
                  ondragstart={(e) => onBlockDragStart(e, t)}
                  style="top:{blockTop(t)}px; height:{Math.max((blockMins(t) / 60) * HOUR_H, 18)}px;"
                  title="{blockLabel(t)} · {t.title}"
                >
                  <button class="block-body" onclick={() => onOpenTask(t.id)}>
                    <span class="block-time">{blockLabel(t)}</span>
                    <span class="block-title">{t.title}</span>
                  </button>
                  <button class="block-x" title="Снять блок" onclick={(e) => { e.stopPropagation(); unschedule(t.id); }}>✕</button>
                  <div class="resize-handle" role="presentation" onmousedown={(e) => startResize(e, t)}></div>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    </div>

    <aside class="backlog card">
      <div class="section-title" style="margin-bottom:8px;">Бэклог</div>
      {#if backlog.length === 0}
        <p class="muted" style="font-size:12px;margin:0;">Все активные задачи уже в расписании</p>
      {:else}
        {#each backlog as t (t.id)}
          <div
            class="backlog-item"
            draggable="true"
            role="listitem"
            ondragstart={(e) => onBacklogDragStart(e, t)}
            title="Перетащите на день и время"
          >{t.title}</div>
        {/each}
      {/if}
      <p class="muted" style="font-size:11px;margin:10px 0 0 0;">
        Перетащите задачу в сетку — блок встанет с шагом {SNAP_MIN} мин.
        Нижняя кромка блока тянется мышью.
      </p>
    </aside>
  </div>
  {:else}
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
  {/if}
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

  /* ===== Неделя ===== */
  .mode-toggle {
    display: flex;
    gap: 4px;
    margin-left: 12px;
  }

  .active-toggle {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    font-weight: 600;
  }

  .week-layout {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }

  .week-main {
    flex: 1;
    min-width: 0;
    padding: 0;
    overflow: hidden;
  }

  .week-head {
    display: grid;
    grid-template-columns: 48px repeat(7, 1fr);
    border-bottom: 1px solid var(--border);
  }

  .week-day-head {
    text-align: center;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .04em;
    color: var(--text-secondary);
    padding: 6px 0;
  }

  .week-day-head.today {
    color: var(--accent);
    font-weight: 700;
  }

  .week-scroll {
    max-height: calc(100vh - 190px);
    overflow-y: auto;
  }

  .week-grid {
    display: grid;
    grid-template-columns: 48px repeat(7, 1fr);
  }

  .hour-gutter {
    border-right: 1px solid var(--border);
  }

  .hour-mark {
    font-size: 10px;
    color: var(--text-secondary);
    text-align: right;
    padding-right: 6px;
    box-sizing: border-box;
    border-top: 1px solid transparent;
    transform: translateY(-6px);
  }

  .week-col {
    position: relative;
    border-right: 1px solid var(--border);
    background-image: linear-gradient(to bottom, var(--border) 1px, transparent 1px);
  }

  .week-col:last-child {
    border-right: none;
  }

  .week-col.today {
    background-color: color-mix(in srgb, var(--accent) 4%, transparent);
  }

  .block {
    position: absolute;
    left: 2px;
    right: 2px;
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-primary));
    border-left: 3px solid var(--accent);
    border-radius: 4px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    cursor: grab;
    z-index: 1;
  }

  .block:hover {
    background: color-mix(in srgb, var(--accent) 26%, var(--bg-primary));
    z-index: 2;
  }

  .block-body {
    flex: 1;
    min-height: 0;
    background: transparent;
    border: none;
    padding: 2px 16px 2px 5px;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    cursor: inherit;
    color: inherit;
  }

  .block-time {
    font-size: 10px;
    color: var(--accent);
    font-weight: 600;
    white-space: nowrap;
  }

  .block-title {
    font-size: 11px;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .block-x {
    position: absolute;
    top: 1px;
    right: 1px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font-size: 10px;
    padding: 1px 4px;
    opacity: 0;
  }

  .block:hover .block-x {
    opacity: 1;
  }

  .resize-handle {
    height: 6px;
    cursor: ns-resize;
    flex-shrink: 0;
  }

  .backlog {
    width: 200px;
    flex-shrink: 0;
    padding: 12px;
  }

  .backlog-item {
    font-size: 12px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 5px;
    cursor: grab;
    background: var(--bg-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .backlog-item:active {
    cursor: grabbing;
  }
</style>
