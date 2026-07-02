<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api/tauri";

  interface ActivityDay {
    date: string;
    minutes: number;
  }

  interface TaskCompletion {
    date: string;
    completed: number;
  }

  let activityDays: ActivityDay[] = $state([]);
  let taskCompletions: TaskCompletion[] = $state([]);
  let error: string | null = $state(null);

  onMount(async () => {
    try {
      activityDays = await api.getActivityByDay();
      taskCompletions = await api.getTaskCompletionsByDay();
    } catch (e) {
      error = String(e);
    }
  });

  function maxMinutes(days: ActivityDay[]) {
    return Math.max(...days.map(d => d.minutes), 1);
  }

  function barColor(minutes: number, max: number) {
    const ratio = minutes / max;
    if (ratio > 0.66) return "#22c55e";
    if (ratio > 0.33) return "#f59e0b";
    return "#94a3b8";
  }
</script>

<div style="padding:4px;max-width:720px;">
  <h2 style="margin-top:0;">Дашборд</h2>

  {#if error}
    <div style="background:#fee2e2;color:#dc2626;padding:8px 12px;border-radius:6px;margin-bottom:12px;">{error}</div>
  {/if}

  <!-- Activity heatmap -->
  <section style="margin-bottom:32px;">
    <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
      Активность по дням (мин)
    </h3>

    {#if activityDays.length === 0}
      <p style="color:var(--text-secondary,#6b7280);font-size:13px;">Нет данных</p>
    {:else}
      {@const max = maxMinutes(activityDays)}
      <div style="display:flex;flex-direction:column;gap:4px;">
        {#each activityDays.slice(-30) as day (day.date)}
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;">
            <span style="width:80px;color:var(--text-secondary,#6b7280);flex-shrink:0;">{day.date}</span>
            <div style="flex:1;background:var(--border,#e5e7eb);border-radius:3px;height:16px;position:relative;">
              <div style="
                width:{Math.round((day.minutes / max) * 100)}%;
                height:100%;
                background:{barColor(day.minutes, max)};
                border-radius:3px;
              "></div>
            </div>
            <span style="width:48px;text-align:right;color:var(--text-secondary,#6b7280);">{day.minutes} мин</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <!-- Task completions -->
  <section>
    <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
      Выполненные задачи по дням
    </h3>

    {#if taskCompletions.length === 0}
      <p style="color:var(--text-secondary,#6b7280);font-size:13px;">Нет данных</p>
    {:else}
      <div style="display:flex;flex-direction:column;gap:4px;">
        {#each taskCompletions.slice(-30) as day (day.date)}
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;">
            <span style="width:80px;color:var(--text-secondary,#6b7280);flex-shrink:0;">{day.date}</span>
            <div style="display:flex;gap:4px;flex-wrap:wrap;">
              {#each Array(day.completed) as _}
                <span style="width:14px;height:14px;background:#6366f1;border-radius:3px;display:inline-block;"></span>
              {/each}
            </div>
            <span style="color:var(--text-secondary,#6b7280);">{day.completed}</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>
