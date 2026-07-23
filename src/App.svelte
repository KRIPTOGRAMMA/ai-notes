<script lang="ts">
  import { taskStore } from "./lib/stores/tasks.svelte";
  import { noteStore } from "./lib/stores/notes.svelte";
  import { projectStore } from "./lib/stores/projects.svelte";
  import { api } from "./lib/api/tauri";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { AppSettings } from "./lib/types";
  import { applyCachedTheme, applyTheme } from "./lib/theme";
  import { parseKeybinds, comboFor, comboMatches, formatCombo, type Keybinds } from "./lib/keybinds";
  import Onboarding from "./views/Onboarding.svelte";
  import Tasks from "./views/Tasks.svelte";
  import Notes from "./views/Notes.svelte";
  import NotesGraph from "./views/NotesGraph.svelte";
  import Settings from "./views/Settings.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import Calendar from "./views/Calendar.svelte";
  import Today from "./views/Today.svelte";
  import SearchOverlay from "./lib/components/SearchOverlay.svelte";
  import PomodoroWidget from "./lib/components/PomodoroWidget.svelte";
  import TrackingWidget from "./lib/components/TrackingWidget.svelte";
  import NotificationPanel from "./lib/components/NotificationPanel.svelte";
  import Icon from "./lib/components/Icon.svelte";
  import "./app.css";

  type View = "today" | "tasks" | "notes" | "graph" | "dashboard" | "calendar" | "settings";
  let activeView: View = $state("tasks");
  let showSearch = $state(false);
  let showNotifications = $state(false);
  let unreadNotifications = $state(0);

  // Онбординг: пока настройки не загружены — ничего не показываем,
  // чтобы главный экран не мелькал перед онбордингом
  let loadedSettings: AppSettings | null = $state(null);
  let showOnboarding = $state(false);
  let isWayland = $state(false);
  let keybinds: Keybinds = $state({});

  // Тема: сначала из кеша (анти-мигание), затем — источник истины из БД.
  applyCachedTheme();

  onMount(async () => {
    // Заметки нужны глобально для поиска (Ctrl+K), даже если раздел ещё не открывали.
    noteStore.load();
    // Проекты нужны модалу задачи из любого раздела (например, из Календаря).
    projectStore.load();
    // Создание в окне быстрого ввода — подхватываем глобально: раздел задач
    // может быть не смонтирован (открыт Календарь/Дашборд), а store общий.
    const unlistenNote = listen("note-created", () => noteStore.load());
    const unlistenTask = listen("task-created", () => taskStore.load());
    void unlistenNote;
    void unlistenTask;
    // Settings.svelte живёт в том же вебвью — обычное window-событие,
    // без Tauri IPC, чтобы переназначенный хоткей заработал сразу, без reload.
    const onKeybindsSaved = (e: Event) => {
      keybinds = parseKeybinds((e as CustomEvent<string>).detail ?? "");
    };
    window.addEventListener("keybinds-saved", onKeybindsSaved);
    try {
      isWayland = await api.isWayland();
      loadedSettings = await api.getSettings();
      showOnboarding = !loadedSettings.onboarding_complete;
      applyTheme(loadedSettings.theme_mode, loadedSettings);
      keybinds = parseKeybinds(loadedSettings.keybinds);
    } catch {
      loadedSettings = null;
      showOnboarding = false;
    }
    void pollUnreadNotifications();
    setInterval(pollUnreadNotifications, 30_000);
  });

  async function pollUnreadNotifications() {
    unreadNotifications = await api.getUnreadNotificationCount().catch(() => 0);
  }

  const NAV: { view: View; label: string; icon: string; actionId: string }[] = [
    { view: "today",     label: "Сегодня",   actionId: "view_today",     icon: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z M12 1v2 M12 21v2 M4.22 4.22l1.42 1.42 M18.36 18.36l1.42 1.42 M1 12h2 M21 12h2 M4.22 19.78l1.42-1.42 M18.36 5.64l1.42-1.42" },
    { view: "tasks",     label: "Задачи",    actionId: "view_tasks",     icon: "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z M9 12l2 2 4-4" },
    { view: "notes",     label: "Заметки",   actionId: "view_notes",     icon: "M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Z M14 3v6h6 M8 14h8 M8 17h5" },
    { view: "graph",     label: "Граф",      actionId: "view_graph",     icon: "M6 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z M18 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z M6 20a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z M17 18a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z M7.5 5.5l6 1 M7 7l8.5 9 M6 16l9.5-9.7" },
    { view: "dashboard", label: "Дашборд",   actionId: "view_dashboard", icon: "M6 20v-4 M12 20V10 M18 20V4" },
    { view: "calendar",  label: "Календарь", actionId: "view_calendar", icon: "M5 5h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z M16 3v4 M8 3v4 M3 11h18" },
    { view: "settings",  label: "Настройки", actionId: "view_settings", icon: "M21 5h-7 M10 5H3 M21 12h-9 M8 12H3 M21 19h-5 M12 19H3 M14 3v4 M8 10v4 M16 17v4" },
  ];

  // Командная палитра: «Сменить тему» — цикл light → dark → system, применяет
  // сразу и сохраняет (та же логика сохранения, что и Settings.svelte::save()).
  async function cycleTheme() {
    if (!loadedSettings) return;
    const order: AppSettings["theme_mode"][] = ["light", "dark", "system"];
    const next = order[(order.indexOf(loadedSettings.theme_mode) + 1) % order.length];
    loadedSettings.theme_mode = next;
    applyTheme(next, loadedSettings);
    await api.saveSettings(loadedSettings);
  }

  let lastActivityPing = 0;
  const paletteCommands = [
    { label: "Новая задача", hint: "Создать задачу", keywords: "новая задача create task", run: () => { activeView = "tasks"; taskStore.requestCreate(); } },
    { label: "Новая заметка", hint: "Создать заметку", keywords: "новая заметка create note", run: () => { activeView = "notes"; } },
    { label: "Заметка дня", hint: "Открыть/создать дневную заметку", keywords: "заметка дня daily note today", run: () => { activeView = "notes"; noteStore.requestDaily(); } },
    { label: "Перейти: Сегодня", keywords: "перейти сегодня go today", run: () => { activeView = "today"; } },
    { label: "Перейти: Задачи", keywords: "перейти задачи go tasks", run: () => { activeView = "tasks"; } },
    { label: "Перейти: Заметки", keywords: "перейти заметки go notes", run: () => { activeView = "notes"; } },
    { label: "Перейти: Граф заметок", keywords: "перейти граф graph notes", run: () => { activeView = "graph"; } },
    { label: "Перейти: Дашборд", keywords: "перейти дашборд go dashboard", run: () => { activeView = "dashboard"; } },
    { label: "Перейти: Календарь", keywords: "перейти календарь go calendar", run: () => { activeView = "calendar"; } },
    { label: "Перейти: Настройки", keywords: "перейти настройки go settings", run: () => { activeView = "settings"; } },
    { label: "Спланировать день", hint: "Календарь-неделя + ИИ-план", keywords: "спланировать день план calendar plan day", run: () => { activeView = "calendar"; taskStore.requestPlanDay(); } },
    { label: "Сменить тему", hint: "Светлая → тёмная → системная", keywords: "сменить тема theme dark light", run: cycleTheme },
  ];

  function pingActivity() {
    const now = Date.now();
    if (now - lastActivityPing < 10_000) return;
    lastActivityPing = now;
    api.recordInput().catch(() => {});
  }
</script>

<svelte:window
  onmousemove={pingActivity}
  onkeydown={(e) => {
    pingActivity();
    if (!e.ctrlKey) return;
    // Ctrl+Shift+N/M — быстрая задача/заметка: не переназначаемы в v0.8.9,
    // держатся в синхроне с OS-уровневым global-shortcut (см. lib.rs).
    if (e.shiftKey && e.code === "KeyN") {
      e.preventDefault();
      api.openQuickCapture("task").catch(() => {});
      return;
    }
    if (e.shiftKey && e.code === "KeyM") {
      e.preventDefault();
      api.openQuickCapture("note").catch(() => {});
      return;
    }
    if (comboMatches(comboFor(keybinds, "palette"), e)) {
      e.preventDefault();
      showSearch = true;
    } else if (comboMatches(comboFor(keybinds, "daily_note"), e)) {
      e.preventDefault();
      activeView = "notes";
      noteStore.requestDaily();
    } else {
      const viewActions: [string, View][] = [
        ["view_today", "today"], ["view_tasks", "tasks"], ["view_notes", "notes"], ["view_dashboard", "dashboard"],
        ["view_calendar", "calendar"], ["view_settings", "settings"], ["view_graph", "graph"],
      ];
      for (const [actionId, view] of viewActions) {
        if (comboMatches(comboFor(keybinds, actionId), e)) {
          e.preventDefault();
          activeView = view;
          break;
        }
      }
    }
  }}
/>

{#if showSearch}
  <SearchOverlay
    commands={paletteCommands}
    onClose={() => showSearch = false}
    onSelectTask={(id) => { activeView = "tasks"; taskStore.requestFocus(id); showSearch = false; }}
    onSelectNote={(id) => { activeView = "notes"; noteStore.requestFocus(id); showSearch = false; }}
  />
{/if}

{#if showOnboarding && loadedSettings}
  <Onboarding
    settings={loadedSettings}
    {isWayland}
    onDone={() => showOnboarding = false}
  />
{:else}
<div class="shell">
  <aside class="sidebar">
    <div class="brand">AI Notes</div>

    <nav class="nav">
      {#each NAV as item (item.view)}
        <button
          class="nav-item"
          class:active={activeView === item.view}
          onclick={() => activeView = item.view}
          title="{item.label} ({formatCombo(comboFor(keybinds, item.actionId))})"
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none"
            stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d={item.icon} />
          </svg>
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <TrackingWidget />

    <PomodoroWidget />

    <button class="nav-item search-item" onclick={() => showSearch = true} title="Поиск (Ctrl+K)">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none"
        stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z M21 21l-4.35-4.35" />
      </svg>
      <span>Поиск</span>
      <kbd>Ctrl K</kbd>
    </button>

    <button class="nav-item bell-item" onclick={() => showNotifications = true} title="Уведомления">
      <Icon name="bell" size={16} />
      <span>Уведомления</span>
      {#if unreadNotifications > 0}
        <span class="unread-badge">{unreadNotifications > 99 ? "99+" : unreadNotifications}</span>
      {/if}
    </button>
  </aside>

  {#if showNotifications}
    <NotificationPanel onClose={() => { showNotifications = false; pollUnreadNotifications(); }} />
  {/if}

  <main class="content">
    {#if taskStore.error}
      <div class="banner">
        <span>{taskStore.error}</span>
        <button class="btn-icon" onclick={() => taskStore.clearError()} style="color:white;">✕</button>
      </div>
    {/if}

    {#if noteStore.error}
      <div class="banner">
        <span>{noteStore.error}</span>
        <button class="btn-icon" onclick={() => noteStore.clearError()} style="color:white;">✕</button>
      </div>
    {/if}

    {#if activeView === "today"}
      <Today onOpenTask={(id) => { activeView = "tasks"; taskStore.requestFocus(id); }} />
    {:else if activeView === "tasks"}
      <Tasks />
    {:else if activeView === "notes"}
      <Notes />
    {:else if activeView === "graph"}
      <NotesGraph onOpenNote={(id) => { activeView = "notes"; noteStore.requestFocus(id); }} />
    {:else if activeView === "settings"}
      <Settings />
    {:else if activeView === "dashboard"}
      <Dashboard onOpenTask={(id) => { activeView = "tasks"; taskStore.requestFocus(id); }} />
    {:else if activeView === "calendar"}
      <Calendar onOpenTask={(id) => { activeView = "tasks"; taskStore.requestFocus(id); }} />
    {/if}
  </main>
</div>
{/if}

<style>
  .shell {
    display: flex;
    height: 100vh;
  }

  .sidebar {
    width: 176px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
  }

  .brand {
    font-size: 13px;
    font-weight: 700;
    padding: 4px 10px 12px 10px;
    letter-spacing: .02em;
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    text-align: left;
  }

  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    font-weight: 600;
  }

  .search-item kbd {
    margin-left: auto;
    font-size: 10px;
    font-family: inherit;
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
  }

  .unread-badge {
    margin-left: auto;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--danger);
    color: #fff;
    font-size: 10px;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
  }

  .banner {
    background: var(--danger);
    color: white;
    padding: 8px 12px;
    border-radius: var(--radius);
    margin-bottom: 10px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
</style>
