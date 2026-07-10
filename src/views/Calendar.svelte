<script lang="ts">
  import { onMount } from "svelte";
  import { taskStore } from "../lib/stores/tasks.svelte";
  import type { Task } from "../lib/types";

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

  function chipStyle(t: Task): string {
    const done = t.status === "Done" || t.status === "Archived";
    if (done) {
      return "background:transparent;border:1px solid var(--border,#e5e7eb);color:var(--text-secondary,#6b7280);text-decoration:line-through;";
    }
    if (t.deadline && new Date(t.deadline) < new Date()) {
      return "background:#dc2626;color:white;";
    }
    return "background:var(--accent,#6366f1);color:white;";
  }

  const MAX_CHIPS = 3;
</script>

<div style="padding:4px;">
  <div style="display:flex;align-items:center;gap:8px;margin-bottom:12px;">
    <h2 style="margin:0;">Календарь</h2>
    <span style="flex:1;"></span>
    <button onclick={() => shiftMonth(-1)} title="Предыдущий месяц">←</button>
    <span style="min-width:140px;text-align:center;font-weight:600;">{MONTHS[month]} {year}</span>
    <button onclick={() => shiftMonth(1)} title="Следующий месяц">→</button>
    <button onclick={goToday}>Сегодня</button>
  </div>

  <div style="display:grid;grid-template-columns:repeat(7,1fr);gap:4px;">
    {#each WEEKDAYS as wd}
      <div style="text-align:center;font-size:12px;color:var(--text-secondary,#6b7280);padding:4px 0;">{wd}</div>
    {/each}

    {#each grid as cell (cell.key)}
      <div style="
        min-height:86px;
        border:1px solid {cell.isToday ? 'var(--accent,#6366f1)' : 'var(--border,#e5e7eb)'};
        border-radius:6px;
        padding:4px;
        opacity:{cell.inMonth ? 1 : 0.45};
        background:var(--bg-card,transparent);
      ">
        <div style="font-size:12px;font-weight:{cell.isToday ? '700' : '400'};
          color:{cell.isToday ? 'var(--accent,#6366f1)' : 'var(--text-secondary,#6b7280)'};margin-bottom:4px;">
          {cell.day}
        </div>
        <div style="display:flex;flex-direction:column;gap:2px;">
          {#each cell.tasks.slice(0, MAX_CHIPS) as t (t.id)}
            <button
              onclick={() => onOpenTask(t.id)}
              title={t.title}
              style="
                display:block;width:100%;text-align:left;
                font-size:11px;padding:2px 5px;border-radius:4px;border:none;cursor:pointer;
                white-space:nowrap;overflow:hidden;text-overflow:ellipsis;
                {chipStyle(t)}
              "
            >{t.title}</button>
          {/each}
          {#if cell.tasks.length > MAX_CHIPS}
            <span style="font-size:11px;color:var(--text-secondary,#6b7280);padding-left:5px;">
              +{cell.tasks.length - MAX_CHIPS} ещё
            </span>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  <p style="font-size:12px;color:var(--text-secondary,#6b7280);margin-top:10px;">
    Задачи разложены по дате дедлайна. Красные — просроченные, зачёркнутые — выполненные. Клик открывает задачу.
  </p>
</div>
