<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api/tauri";
  import { projectStore } from "../lib/stores/projects.svelte";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import type { AppSettings, DayCompletion } from "../lib/types";

  let { onOpenTask }: { onOpenTask: (id: string) => void } = $props();

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

  interface PomodoroStats {
    today: number;
    week: number;
    task_streak: number;
    pomodoro_streak: number;
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
  let pomodoroStats: PomodoroStats | null = $state(null);

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

  // Метки для КАТЕГОРИЙ ПРИЛОЖЕНИЙ (glob-правила трекинга — фиксированный набор).
  // Категории задач с v0.6.3 пользовательские и берутся из categoryStore.
  const CATEGORY_LABELS: Record<string, string> = {
    Work: "Работа",
    Study: "Учёба",
    Home: "Дом",
    Health: "Здоровье",
    Other: "Другое",
  };

  const donutData = $derived.by(() => {
    // Порядок и цвета — из таблицы категорий; легаси-значения (категория
    // удалена, задачи в истории остались) — в хвост серым.
    const known = categoryStore.categories.map(c => ({
      category: c.id,
      label: c.name,
      color: c.color,
      count: categories.find(x => x.category === c.id)?.count ?? 0,
    }));
    const knownIds = new Set(categoryStore.categories.map(c => c.id));
    const legacy = categories
      .filter(x => !knownIds.has(x.category))
      .map(x => ({ category: x.category, label: x.category, color: "#888888", count: x.count }));
    return [...known, ...legacy].filter(d => d.count > 0);
  });
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

  // --- Год в квадратиках (v0.6.5): выполненные задачи по локальным дням ---
  const YEAR_DAYS = 365;

  function localKey(d: Date): string {
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
  }

  const calendar = $derived.by(() => {
    const byDate = new Map(taskCompletions.map(c => [c.date, c.completed]));
    const today = new Date();
    const days: { date: string; count: number }[] = [];
    for (let i = YEAR_DAYS - 1; i >= 0; i--) {
      const d = new Date(today.getFullYear(), today.getMonth(), today.getDate() - i);
      const key = localKey(d);
      days.push({ date: key, count: byDate.get(key) ?? 0 });
    }
    // Пустые ячейки в начале, чтобы колонки-недели начинались с понедельника
    const first = new Date(today.getFullYear(), today.getMonth(), today.getDate() - (YEAR_DAYS - 1));
    const lead = (first.getDay() + 6) % 7;
    return { lead, days };
  });

  const calMax = $derived(Math.max(1, ...taskCompletions.map(c => c.completed)));
  const CAL_MIX = [0, 25, 45, 70, 95]; // проценты акцента по уровням

  function calLevel(count: number): number {
    if (count <= 0) return 0;
    const r = count / calMax;
    return r > 0.75 ? 4 : r > 0.5 ? 3 : r > 0.25 ? 2 : 1;
  }

  function fmtDay(date: string): string {
    return new Date(date + "T00:00:00").toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
  }

  // Тултип (hover, быстрый превью) и попап (клик, с переходом к задаче) —
  // список выполненных задач дня подгружается лениво и кэшируется по дате.
  let calTip: { date: string; count: number; completions: DayCompletion[]; x: number; y: number } | null = $state(null);
  let calPopup: { date: string; count: number; completions: DayCompletion[] } | null = $state(null);
  const dayCompletionsCache = new Map<string, DayCompletion[]>();

  async function loadDayCompletions(day: { date: string; count: number }): Promise<DayCompletion[]> {
    if (day.count === 0) return [];
    const cached = dayCompletionsCache.get(day.date);
    if (cached) return cached;
    const completions = await api.getCompletionsForDay(day.date).catch(() => []);
    dayCompletionsCache.set(day.date, completions);
    return completions;
  }

  async function showCalTip(e: MouseEvent, day: { date: string; count: number }) {
    const cell = e.currentTarget as HTMLElement;
    const x = cell.offsetLeft;
    const y = cell.offsetTop;
    const completions = await loadDayCompletions(day);
    calTip = { date: day.date, count: day.count, completions, x, y };
  }

  async function openCalPopup(day: { date: string; count: number }) {
    calTip = null;
    const completions = await loadDayCompletions(day);
    calPopup = { date: day.date, count: day.count, completions };
  }

  function openTaskFromPopup(id: string) {
    calPopup = null;
    onOpenTask(id);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && calPopup) calPopup = null;
  }

  // --- Heatmap «час × день недели» (v0.6.5) ---
  let hourly: { weekday: number; hour: number; minutes: number }[] = $state([]);
  const HOURS = Array.from({ length: 24 }, (_, i) => i);
  const WEEKDAY_LABELS = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
  const heatMax = $derived(Math.max(1, ...hourly.map(c => c.minutes)));

  // row 0 = Пн; в данных weekday по strftime('%w'): 0 = Вс
  function heatMinutes(row: number, hour: number): number {
    const w = (row + 1) % 7;
    return hourly.find(c => c.weekday === w && c.hour === hour)?.minutes ?? 0;
  }

  function heatStyle(mins: number): string {
    if (mins <= 0) return "";
    const p = Math.round(10 + (mins / heatMax) * 85);
    return `background: color-mix(in srgb, var(--accent) ${p}%, transparent);`;
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
        await loadProjectTime();
        await categoryStore.load(); // имена/цвета категорий задач для пончика
        hourly = await api.getHourlyActivity(56); // heatmap: последние 8 недель
        pomodoroStats = await api.getPomodoroStats();

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

  let projectTime: { name: string; mins: number }[] = $state([]);
  async function loadProjectTime() {
    const now = new Date();
    const weekAgo = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 7);
    const result: { name: string; mins: number }[] = [];
    for (const p of projectStore.active) {
      try {
        const secs = await api.getProjectSeconds(p.id, weekAgo.toISOString());
        if (secs >= 60) result.push({ name: p.name, mins: Math.round(secs / 60) });
      } catch {}
    }
    projectTime = result.sort((a, b) => b.mins - a.mins);
  }
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
                  stroke={seg.color}
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
                <span class="swatch" style="background:{seg.color};"></span>
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

    <!-- Помодоро: статистика и стрики -->
    {#if pomodoroStats}
      <section class="card panel">
        <h3 class="section-title">Помодоро</h3>
        <ul class="goals">
          <li class="goal-item">
            <div class="goal-row">
              <span class="goal-metric">сегодня</span>
              <span class="goal-val muted">{pomodoroStats.today}</span>
            </div>
            <div class="goal-row">
              <span class="goal-metric">за неделю</span>
              <span class="goal-val muted">{pomodoroStats.week}</span>
            </div>
            <div class="goal-row">
              <span class="goal-metric">стрик задач</span>
              <span class="goal-val muted">{pomodoroStats.task_streak} дн.</span>
            </div>
            <div class="goal-row">
              <span class="goal-metric">стрик помидоров</span>
              <span class="goal-val muted">{pomodoroStats.pomodoro_streak} дн.</span>
            </div>
          </li>
        </ul>
      </section>
    {/if}

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
          Минуты — по трекингу задач проекта за период.
        </p>
      </section>
    {/if}

    {#if projectTime.length > 0}
      <section class="card panel">
        <h3 class="section-title">Время по проектам (7 дней)</h3>
        <ul class="goals">
          {#each projectTime as pt}
            <li class="goal-item">
              <div class="goal-head">
                <span class="goal-name">{pt.name}</span>
              </div>
              <div class="goal-row">
                <span class="goal-val muted">{pt.mins} мин</span>
              </div>
            </li>
          {/each}
        </ul>
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

    <!-- Год в квадратиках: выполненные задачи -->
    <section class="card panel wide">
      <h3 class="section-title">Выполненные задачи за год</h3>

      {#if taskCompletions.length === 0}
        <div class="empty">Нет данных</div>
      {:else}
        <div class="cal-wrap" onmouseleave={() => calTip = null} role="presentation">
          <div class="cal-grid">
            {#each Array(calendar.lead) as _, i (i)}
              <span class="cal-cell lead"></span>
            {/each}
            {#each calendar.days as day (day.date)}
              <button
                type="button"
                class="cal-cell"
                data-date={day.date}
                data-count={day.count}
                style={day.count > 0
                  ? `background: color-mix(in srgb, var(--accent) ${CAL_MIX[calLevel(day.count)]}%, transparent);`
                  : ""}
                onmouseenter={(e) => showCalTip(e, day)}
                onclick={() => openCalPopup(day)}
                aria-label="{fmtDay(day.date)}: {day.count}"
              ></button>
            {/each}
          </div>
          {#if calTip}
            <div class="cal-tip" style="left:{Math.min(calTip.x, 640)}px; top:{calTip.y + 16}px;">
              <div class="cal-tip-head">{fmtDay(calTip.date)} — {calTip.count > 0 ? `выполнено: ${calTip.count}` : "пусто"}</div>
              {#each calTip.completions as c (c.id)}
                <div class="cal-tip-item">• {c.title}</div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </section>

    <!-- Heatmap: в какие часы реально работается -->
    <section class="card panel wide">
      <h3 class="section-title">Активность по часам (8 недель)</h3>

      {#if hourly.length === 0}
        <div class="empty">Нет данных</div>
      {:else}
        <div class="heat">
          {#each WEEKDAY_LABELS as label, row (label)}
            <span class="heat-label">{label}</span>
            {#each HOURS as h (h)}
              {@const mins = heatMinutes(row, h)}
              <span
                class="heat-cell"
                style={heatStyle(mins)}
                title="{label} {String(h).padStart(2, '0')}:00 — {mins} мин"
              ></span>
            {/each}
          {/each}
          <span class="heat-label"></span>
          {#each HOURS as h (h)}
            <span class="heat-hour">{h % 6 === 0 ? h : ""}</span>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

<svelte:window onkeydown={handleKeydown} />

{#if calPopup}
  <div role="dialog" aria-modal="true" class="overlay backdrop" onclick={(e) => { if (e.target === e.currentTarget) calPopup = null; }}>
    <div class="modal dialog cal-popup">
      <h2 class="dialog-title">{fmtDay(calPopup.date)} — {calPopup.count > 0 ? `выполнено: ${calPopup.count}` : "пусто"}</h2>
      {#if calPopup.completions.length === 0}
        <div class="empty">Нет выполненных задач в этот день</div>
      {:else}
        <ul class="cal-popup-list">
          {#each calPopup.completions as c (c.id)}
            <li>
              <button type="button" class="cal-popup-item" onclick={() => openTaskFromPopup(c.id)}>{c.title}</button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="actions">
        <button class="btn-ghost" onclick={() => calPopup = null}>Закрыть</button>
      </div>
    </div>
  </div>
{/if}

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

  /* Год в квадратиках: колонки-недели, строки Пн..Вс */
  .cal-wrap {
    position: relative;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .cal-grid {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(7, 10px);
    gap: 2px;
    width: max-content;
  }

  .cal-cell {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--bg-secondary);
    border: none;
    padding: 0;
    cursor: pointer;
  }

  .cal-cell.lead { background: transparent; }

  .cal-tip {
    position: absolute;
    z-index: 5;
    max-width: 280px;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
    padding: 6px 10px;
    font-size: 12px;
    pointer-events: none;
  }

  .cal-tip-head { font-weight: 600; }

  .cal-tip-item {
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Heatmap час × день недели */
  .heat {
    display: grid;
    grid-template-columns: 24px repeat(24, 1fr);
    gap: 2px;
    align-items: center;
  }

  .heat-label {
    font-size: 10px;
    color: var(--text-secondary);
  }

  .heat-cell {
    aspect-ratio: 1 / 1;
    min-width: 0;
    border-radius: 2px;
    background: var(--bg-secondary);
  }

  .heat-hour {
    font-size: 9px;
    color: var(--text-secondary);
    text-align: left;
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

  .backdrop {
    align-items: center;
    padding: 16px;
  }

  .dialog {
    width: 100%;
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

  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .cal-popup {
    max-width: 420px;
  }

  .cal-popup-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 320px;
    overflow-y: auto;
  }

  .cal-popup-item {
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    padding: 6px 8px;
    border-radius: var(--radius);
    font-size: 13px;
    cursor: pointer;
  }

  .cal-popup-item:hover {
    background: var(--bg-secondary);
  }
</style>
