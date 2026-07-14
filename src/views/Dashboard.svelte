<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api/tauri";
  import type { AppSettings } from "../lib/types";

  interface ActivityDay {
    date: string;
    minutes: number;
  }

  interface TaskCompletion {
    date: string;
    completed: number;
  }

  interface CategoryCount {
    category: string;
    count: number;
  }

  interface ActiveIdleRatio {
    today_active: number;
    today_idle: number;
    week_active: number;
    week_idle: number;
  }

  interface InsightPayload {
    result: string | null;
    error: string | null;
  }

  interface SummaryPayload {
    kind: "day" | "week";
    result: string | null;
    error: string | null;
  }

  let activityDays: ActivityDay[] = $state([]);
  let taskCompletions: TaskCompletion[] = $state([]);
  let categories: CategoryCount[] = $state([]);
  let ratio: ActiveIdleRatio | null = $state(null);
  let settings: AppSettings | null = $state(null);
  let error: string | null = $state(null);

  let insightText: string | null = $state(null);
  let insightError: string | null = $state(null);
  let insightPending = $state(false);

  let summaryText: string | null = $state(null);
  let summaryError: string | null = $state(null);
  let summaryPending: "day" | "week" | null = $state(null);
  let summaryKind: "day" | "week" | null = $state(null);

  // Фиксированный порядок категорий = порядок слотов палитры (CVD-безопасный).
  const CATEGORY_ORDER = ["Work", "Study", "Home", "Health", "Other"] as const;
  const CATEGORY_LABELS: Record<string, string> = {
    Work: "Работа",
    Study: "Учёба",
    Home: "Дом",
    Health: "Здоровье",
    Other: "Другое",
  };

  const donutData = $derived(
    CATEGORY_ORDER
      .map(cat => ({
        category: cat,
        label: CATEGORY_LABELS[cat],
        count: categories.find(c => c.category === cat)?.count ?? 0,
      }))
      .filter(d => d.count > 0)
  );
  const donutTotal = $derived(donutData.reduce((s, d) => s + d.count, 0));

  const R = 45;
  const CIRC = 2 * Math.PI * R;
  const SEG_GAP = 2; // px просвета между сегментами

  // Сегменты пончика: длина дуги минус зазор, смещение — накопленное.
  const donutSegments = $derived.by(() => {
    const gap = donutData.length > 1 ? SEG_GAP : 0;
    let start = 0;
    return donutData.map(d => {
      const arc = (d.count / donutTotal) * CIRC;
      const seg = {
        ...d,
        dash: Math.max(arc - gap, 0.1),
        offset: -(start + gap / 2),
        percent: Math.round((d.count / donutTotal) * 100),
      };
      start += arc;
      return seg;
    });
  });

  function pct(active: number, idle: number): number | null {
    const total = active + idle;
    return total > 0 ? Math.round((active / total) * 100) : null;
  }

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];

    (async () => {
      try {
        activityDays = await api.getActivityByDay();
        taskCompletions = await api.getTaskCompletionsByDay();
        categories = await api.getCategoryDistribution();
        ratio = await api.getActiveIdleRatio();
        settings = await api.getSettings();
      } catch (e) {
        error = String(e);
      }

      unlisteners.push(await listen<InsightPayload>("dashboard-insight", (event) => {
        insightPending = false;
        insightText = event.payload.result;
        insightError = event.payload.error;
      }));

      unlisteners.push(await listen<SummaryPayload>("period-summary", (event) => {
        summaryPending = null;
        summaryKind = event.payload.kind;
        summaryText = event.payload.result;
        summaryError = event.payload.error;
      }));
    })();

    return () => unlisteners.forEach(u => u());
  });

  async function refreshSummary(kind: "day" | "week") {
    summaryPending = kind;
    summaryError = null;
    try {
      await (kind === "day" ? api.summarizeDay() : api.summarizeWeek());
    } catch (e) {
      summaryPending = null;
      summaryError = String(e);
    }
  }

  async function refreshInsight() {
    insightPending = true;
    insightError = null;
    try {
      await api.dashboardInsight();
    } catch (e) {
      insightPending = false;
      insightError = String(e);
    }
  }

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

<div class="dashboard-viz" style="padding:4px;max-width:720px;">
  <h2 style="margin-top:0;">Дашборд</h2>

  {#if error}
    <div style="background:#fee2e2;color:#dc2626;padding:8px 12px;border-radius:6px;margin-bottom:12px;">{error}</div>
  {/if}

  <!-- Донат по категориям + актив/простой -->
  <div style="display:flex;gap:32px;flex-wrap:wrap;margin-bottom:32px;">
    <section style="flex:1;min-width:260px;">
      <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
        Выполнено по категориям
      </h3>

      {#if donutData.length === 0}
        <p style="color:var(--text-secondary,#6b7280);font-size:13px;">Нет выполненных задач</p>
      {:else}
        <div style="display:flex;align-items:center;gap:20px;">
          <svg viewBox="0 0 120 120" width="140" height="140" role="img" aria-label="Выполненные задачи по категориям">
            <g transform="rotate(-90 60 60)">
              {#each donutSegments as seg (seg.category)}
                <circle
                  cx="60" cy="60" r={R}
                  fill="none"
                  stroke="var(--cat-{seg.category.toLowerCase()})"
                  stroke-width="16"
                  stroke-dasharray="{seg.dash} {CIRC - seg.dash}"
                  stroke-dashoffset={seg.offset}
                >
                  <title>{seg.label}: {seg.count} ({seg.percent}%)</title>
                </circle>
              {/each}
            </g>
            <text x="60" y="57" text-anchor="middle" style="font-size:22px;font-weight:600;fill:var(--text-primary,#111827);">{donutTotal}</text>
            <text x="60" y="74" text-anchor="middle" style="font-size:10px;fill:var(--text-secondary,#6b7280);">всего</text>
          </svg>

          <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:6px;font-size:13px;">
            {#each donutSegments as seg (seg.category)}
              <li style="display:flex;align-items:center;gap:8px;">
                <span style="width:12px;height:12px;border-radius:3px;background:var(--cat-{seg.category.toLowerCase()});flex-shrink:0;"></span>
                <span>{seg.label}</span>
                <span style="color:var(--text-secondary,#6b7280);">{seg.count} · {seg.percent}%</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </section>

    <section style="flex:1;min-width:260px;">
      <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
        Активное время
      </h3>

      {#if ratio}
        {#each [
          { label: "Сегодня", value: pct(ratio.today_active, ratio.today_idle), active: ratio.today_active },
          { label: "Неделя", value: pct(ratio.week_active, ratio.week_idle), active: ratio.week_active },
        ] as row (row.label)}
          <div style="margin-bottom:14px;">
            <div style="display:flex;justify-content:space-between;font-size:13px;margin-bottom:4px;">
              <span>{row.label}</span>
              <span style="color:var(--text-secondary,#6b7280);">
                {#if row.value === null}
                  нет данных
                {:else}
                  {row.value}% актив · {Math.round(row.active / 60)} мин
                {/if}
              </span>
            </div>
            <div style="background:var(--border,#e5e7eb);border-radius:3px;height:8px;">
              <div style="width:{row.value ?? 0}%;height:100%;background:var(--accent,#6366f1);border-radius:3px;"></div>
            </div>
          </div>
        {/each}
      {:else}
        <p style="color:var(--text-secondary,#6b7280);font-size:13px;">Нет данных</p>
      {/if}
    </section>
  </div>

  <!-- ИИ-инсайт -->
  <section style="margin-bottom:32px;">
    <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
      ИИ-инсайт
    </h3>

    {#if settings && settings.ai_provider === "none"}
      <p style="color:var(--text-secondary,#6b7280);font-size:13px;">
        ИИ отключён — включите провайдера в Настройках, чтобы получать инсайты.
      </p>
    {:else}
      <div style="display:flex;align-items:flex-start;gap:12px;">
        <button onclick={refreshInsight} disabled={insightPending}
          style="padding:6px 14px;border-radius:6px;border:1px solid var(--border,#e5e7eb);background:var(--bg-secondary,#f5f5f5);cursor:pointer;font-size:13px;flex-shrink:0;">
          {insightPending ? "Думаю…" : "Обновить"}
        </button>
        <div style="font-size:14px;line-height:1.5;">
          {#if insightError}
            <span style="color:#dc2626;">{insightError}</span>
          {:else if insightText}
            {insightText}
          {:else if !insightPending}
            <span style="color:var(--text-secondary,#6b7280);">Нажмите «Обновить», чтобы получить короткий разбор вашей продуктивности.</span>
          {/if}
        </div>
      </div>
    {/if}
  </section>

  <!-- Резюме дня/недели -->
  <section style="margin-bottom:32px;">
    <h3 style="font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;margin:0 0 12px 0;">
      Резюме
    </h3>

    {#if settings && settings.ai_provider === "none"}
      <p style="color:var(--text-secondary,#6b7280);font-size:13px;">
        ИИ отключён — резюме недоступно.
      </p>
    {:else}
      <div style="display:flex;align-items:flex-start;gap:12px;">
        <div style="display:flex;gap:8px;flex-shrink:0;">
          <button onclick={() => refreshSummary("day")} disabled={summaryPending !== null}
            style="padding:6px 14px;border-radius:6px;border:1px solid var(--border,#e5e7eb);background:var(--bg-secondary,#f5f5f5);cursor:pointer;font-size:13px;">
            {summaryPending === "day" ? "Думаю…" : "День"}
          </button>
          <button onclick={() => refreshSummary("week")} disabled={summaryPending !== null}
            style="padding:6px 14px;border-radius:6px;border:1px solid var(--border,#e5e7eb);background:var(--bg-secondary,#f5f5f5);cursor:pointer;font-size:13px;">
            {summaryPending === "week" ? "Думаю…" : "Неделя"}
          </button>
        </div>
        <div style="font-size:14px;line-height:1.5;">
          {#if summaryError}
            <span style="color:#dc2626;">{summaryError}</span>
          {:else if summaryText}
            <span style="color:var(--text-secondary,#6b7280);font-size:12px;display:block;margin-bottom:2px;">
              {summaryKind === "day" ? "За день" : "За неделю"}
            </span>
            {summaryText}
          {:else if summaryPending === null}
            <span style="color:var(--text-secondary,#6b7280);">Резюме дня или недели: что сделано и сколько времени было активным.</span>
          {/if}
        </div>
      </div>
    {/if}
  </section>

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

<!-- Палитра категорий (--cat-*) задана глобально в app.css и разделяется
     с чипами категорий в списке задач. -->
