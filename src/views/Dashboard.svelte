<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api/tauri";
  import { projectStore } from "../lib/stores/projects.svelte";
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

  // Приложения: топ по активным минутам + время по категориям (правила из Настроек)
  let appUsage: { app: string; minutes: number }[] = $state([]);
  let appCategories: { category: string; minutes: number }[] = $state([]);
  let appPeriod: 1 | 7 = $state(1);

  async function loadAppUsage(days: 1 | 7) {
    appPeriod = days;
    try {
      appUsage = await api.getAppUsage(days);
      appCategories = await api.getAppCategoryTime(days);
    } catch {
      appUsage = [];
      appCategories = [];
    }
  }

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
        await loadAppUsage(1);
        await projectStore.load(); // свежий прогресс целей за период

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

  // Цели проектов: активные проекты с заданной целью (задачи и/или минуты)
  const goalProjects = $derived(
    projectStore.active.filter(p => p.goal_tasks != null || p.goal_mins != null)
  );

  const goalPct = (done: number, target: number) =>
    Math.min(100, Math.round((done / Math.max(target, 1)) * 100));
</script>

<div class="dash">
  <h2 class="page-title" style="margin-bottom:14px;">Дашборд</h2>

  {#if error}
    <div class="alert">{error}</div>
  {/if}

  <div class="grid">
    <!-- Донат по категориям -->
    <section class="card panel">
      <h3 class="section-title">Выполнено по категориям</h3>

      {#if donutData.length === 0}
        <div class="empty">Нет выполненных задач</div>
      {:else}
        <div class="donut-row">
          <svg viewBox="0 0 120 120" width="132" height="132" role="img" aria-label="Выполненные задачи по категориям">
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
            <text x="60" y="57" text-anchor="middle" class="donut-total">{donutTotal}</text>
            <text x="60" y="74" text-anchor="middle" class="donut-caption">всего</text>
          </svg>

          <ul class="legend">
            {#each donutSegments as seg (seg.category)}
              <li>
                <span class="swatch" style="background:var(--cat-{seg.category.toLowerCase()});"></span>
                <span>{seg.label}</span>
                <span class="muted">{seg.count} · {seg.percent}%</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </section>

    <!-- Актив/простой -->
    <section class="card panel">
      <h3 class="section-title">Активное время</h3>

      {#if ratio}
        {#each [
          { label: "Сегодня", value: pct(ratio.today_active, ratio.today_idle), active: ratio.today_active },
          { label: "Неделя", value: pct(ratio.week_active, ratio.week_idle), active: ratio.week_active },
        ] as row (row.label)}
          <div class="ratio-row">
            <div class="ratio-head">
              <span>{row.label}</span>
              <span class="muted">
                {#if row.value === null}
                  нет данных
                {:else}
                  {row.value}% актив · {Math.round(row.active / 60)} мин
                {/if}
              </span>
            </div>
            <div class="track">
              <div class="fill" style="width:{row.value ?? 0}%;"></div>
            </div>
          </div>
        {/each}
      {:else}
        <div class="empty">Нет данных</div>
      {/if}
    </section>

    <!-- ИИ-инсайт -->
    <section class="card panel">
      <h3 class="section-title">ИИ-инсайт</h3>

      {#if settings && settings.ai_provider === "none"}
        <p class="muted" style="margin:0;">ИИ отключён — включите провайдера в Настройках, чтобы получать инсайты.</p>
      {:else}
        <div class="ai-row">
          <button onclick={refreshInsight} disabled={insightPending}>
            {insightPending ? "Думаю…" : "Обновить"}
          </button>
          <div class="ai-text">
            {#if insightError}
              <span style="color:var(--danger);">{insightError}</span>
            {:else if insightText}
              {insightText}
            {:else if !insightPending}
              <span class="muted">Нажмите «Обновить», чтобы получить короткий разбор вашей продуктивности.</span>
            {/if}
          </div>
        </div>
      {/if}
    </section>

    <!-- Резюме дня/недели -->
    <section class="card panel">
      <h3 class="section-title">Резюме</h3>

      {#if settings && settings.ai_provider === "none"}
        <p class="muted" style="margin:0;">ИИ отключён — резюме недоступно.</p>
      {:else}
        <div class="ai-row">
          <div class="btn-group">
            <button onclick={() => refreshSummary("day")} disabled={summaryPending !== null}>
              {summaryPending === "day" ? "Думаю…" : "День"}
            </button>
            <button onclick={() => refreshSummary("week")} disabled={summaryPending !== null}>
              {summaryPending === "week" ? "Думаю…" : "Неделя"}
            </button>
          </div>
          <div class="ai-text">
            {#if summaryError}
              <span style="color:var(--danger);">{summaryError}</span>
            {:else if summaryText}
              <span class="muted" style="font-size:11px;display:block;margin-bottom:2px;">
                {summaryKind === "day" ? "За день" : "За неделю"}
              </span>
              {summaryText}
            {:else if summaryPending === null}
              <span class="muted">Резюме дня или недели: что сделано и сколько времени было активным.</span>
            {/if}
          </div>
        </div>
      {/if}
    </section>

    <!-- Цели проектов (только если у кого-то задана цель) -->
    {#if goalProjects.length > 0}
      <section class="card panel">
        <h3 class="section-title">Цели проектов</h3>
        <ul class="goals">
          {#each goalProjects as p (p.id)}
            <li class="goal-item">
              <div class="goal-head">
                <span class="goal-name">{p.name}</span>
                <span class="muted">{p.goal_period === "month" ? "месяц" : "неделя"}</span>
              </div>
              {#if p.goal_tasks != null}
                <div class="goal-row">
                  <span class="goal-metric">задачи</span>
                  <div class="track">
                    <div class="fill" class:done={p.goal_done_tasks >= p.goal_tasks}
                      style="width:{goalPct(p.goal_done_tasks, p.goal_tasks)}%;"></div>
                  </div>
                  <span class="goal-val muted">{p.goal_done_tasks}/{p.goal_tasks}</span>
                </div>
              {/if}
              {#if p.goal_mins != null}
                <div class="goal-row">
                  <span class="goal-metric">минуты</span>
                  <div class="track">
                    <div class="fill" class:done={p.goal_done_mins >= p.goal_mins}
                      style="width:{goalPct(p.goal_done_mins, p.goal_mins)}%;"></div>
                  </div>
                  <span class="goal-val muted">{p.goal_done_mins}/{p.goal_mins}</span>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="muted" style="font-size:11px;margin:8px 0 0 0;">
          Минуты — по прошедшим тайм-блокам задач проекта за период.
        </p>
      </section>
    {/if}

    <!-- Приложения (только если провайдер окон что-то записал) -->
    {#if appUsage.length > 0}
      {@const maxApp = Math.max(...appUsage.map(a => a.minutes), 1)}
      <section class="card panel wide">
        <div class="apps-head">
          <h3 class="section-title" style="margin:0;">Приложения</h3>
          <div class="btn-group">
            <button class:active-toggle={appPeriod === 1} onclick={() => loadAppUsage(1)}>Сегодня</button>
            <button class:active-toggle={appPeriod === 7} onclick={() => loadAppUsage(7)}>Неделя</button>
          </div>
        </div>

        <div class="apps-cols">
          <div class="rows" style="flex:1;min-width:0;">
            {#each appUsage as a (a.app)}
              <div class="bar-row">
                <span class="bar-date" title={a.app}>{a.app}</span>
                <div class="track tall">
                  <div class="fill" style="width:{Math.round((a.minutes / maxApp) * 100)}%;"></div>
                </div>
                <span class="bar-val">{a.minutes} мин</span>
              </div>
            {/each}
          </div>

          <ul class="legend">
            {#each appCategories as c (c.category)}
              <li>
                <span class="swatch" style="background:var(--cat-{c.category.toLowerCase()});"></span>
                <span>{CATEGORY_LABELS[c.category] ?? c.category}</span>
                <span class="muted">{c.minutes} мин</span>
              </li>
            {/each}
          </ul>
        </div>
        <p class="muted" style="font-size:11px;margin:8px 0 0 0;">
          Категории — по правилам «класс окна → категория» в Настройках → Мониторинг.
        </p>
      </section>
    {/if}

    <!-- Активность по дням -->
    <section class="card panel wide">
      <h3 class="section-title">Активность по дням (мин)</h3>

      {#if activityDays.length === 0}
        <div class="empty">Нет данных</div>
      {:else}
        {@const max = maxMinutes(activityDays)}
        <div class="rows">
          {#each activityDays.slice(-30) as day (day.date)}
            <div class="bar-row">
              <span class="bar-date">{day.date}</span>
              <div class="track tall">
                <div class="fill" style="width:{Math.round((day.minutes / max) * 100)}%;"></div>
              </div>
              <span class="bar-val">{day.minutes} мин</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Выполненные задачи по дням -->
    <section class="card panel wide">
      <h3 class="section-title">Выполненные задачи по дням</h3>

      {#if taskCompletions.length === 0}
        <div class="empty">Нет данных</div>
      {:else}
        <div class="rows">
          {#each taskCompletions.slice(-30) as day (day.date)}
            <div class="bar-row">
              <span class="bar-date">{day.date}</span>
              <div class="dots">
                {#each Array(day.completed) as _}
                  <span class="dot"></span>
                {/each}
              </div>
              <span class="muted">{day.completed}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .dash {
    max-width: 860px;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .panel {
    padding: 14px 16px;
    min-width: 0;
  }

  .panel.wide {
    grid-column: 1 / -1;
  }

  @media (max-width: 720px) {
    .grid { grid-template-columns: 1fr; }
  }

  .donut-row {
    display: flex;
    align-items: center;
    gap: 18px;
  }

  .donut-total {
    font-size: 22px;
    font-weight: 600;
    fill: var(--text-primary);
  }

  .donut-caption {
    font-size: 10px;
    fill: var(--text-secondary);
  }

  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 13px;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .swatch {
    width: 11px;
    height: 11px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .ratio-row { margin-bottom: 12px; }
  .ratio-row:last-child { margin-bottom: 0; }

  .ratio-head {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
    margin-bottom: 4px;
  }

  .track {
    background: var(--bg-secondary);
    border-radius: 3px;
    height: 7px;
    overflow: hidden;
  }

  .track.tall { height: 14px; flex: 1; }

  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
  }

  .fill.done { background: var(--success); }

  .goals {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .goal-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 13px;
    margin-bottom: 4px;
  }

  .goal-name { font-weight: 600; }

  .goal-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 3px;
  }

  .goal-metric {
    font-size: 11px;
    color: var(--text-secondary);
    width: 48px;
    flex-shrink: 0;
  }

  .goal-row .track { flex: 1; }

  .goal-val {
    font-size: 11px;
    min-width: 52px;
    text-align: right;
    flex-shrink: 0;
  }

  .ai-row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .apps-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }

  .apps-cols {
    display: flex;
    gap: 24px;
    align-items: flex-start;
  }

  .active-toggle {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }

  .btn-group {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .ai-text {
    font-size: 13px;
    line-height: 1.55;
    padding-top: 4px;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .bar-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .bar-date {
    width: 78px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .bar-val {
    width: 52px;
    text-align: right;
    color: var(--text-secondary);
  }

  .dots {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
  }

  .dot {
    width: 12px;
    height: 12px;
    background: var(--accent);
    border-radius: 3px;
    display: inline-block;
  }
</style>
