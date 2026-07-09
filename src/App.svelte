<script lang="ts">
  import { taskStore } from "./lib/stores/tasks.svelte";
  import { noteStore } from "./lib/stores/notes.svelte";
  import { api } from "./lib/api/tauri";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { AppSettings } from "./lib/types";
  import { applyCachedTheme, applyTheme } from "./lib/theme";
  import Onboarding from "./views/Onboarding.svelte";
  import Tasks from "./views/Tasks.svelte";
  import Notes from "./views/Notes.svelte";
  import Settings from "./views/Settings.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import SearchOverlay from "./lib/components/SearchOverlay.svelte";
  import "./app.css";

  type View = "tasks" | "notes" | "dashboard" | "settings";
  let activeView: View = $state("tasks");
  let showSearch = $state(false);

  // Онбординг: пока настройки не загружены — ничего не показываем,
  // чтобы главный экран не мелькал перед онбордингом
  let loadedSettings: AppSettings | null = $state(null);
  let showOnboarding = $state(false);
  let isWayland = $state(false);

  // Тема: сначала из кеша (анти-мигание), затем — источник истины из БД.
  applyCachedTheme();

  onMount(async () => {
    // Заметки нужны глобально для поиска (Ctrl+K), даже если раздел ещё не открывали.
    noteStore.load();
    // Заметка, созданная в окне быстрого ввода — подхватываем в список.
    const unlistenNote = listen("note-created", () => noteStore.load());
    void unlistenNote;
    try {
      isWayland = await api.isWayland();
      loadedSettings = await api.getSettings();
      showOnboarding = !loadedSettings.onboarding_complete;
      applyTheme(loadedSettings.theme_mode, loadedSettings);
    } catch {
      loadedSettings = null;
      showOnboarding = false;
    }
  });

  // Ctrl+1..4 переключают разделы в порядке шапки.
  const viewOrder: View[] = ["tasks", "notes", "dashboard", "settings"];

  let lastActivityPing = 0;
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
    // Ctrl+Shift+N — быстрая задача, Ctrl+Shift+M — быстрая заметка
    if (e.shiftKey && e.code === "KeyN") {
      e.preventDefault();
      api.openQuickCapture("task").catch(() => {});
    } else if (e.shiftKey && e.code === "KeyM") {
      e.preventDefault();
      api.openQuickCapture("note").catch(() => {});
    } else if (e.code === "KeyK") {
      e.preventDefault();
      showSearch = true;
    } else if (!e.shiftKey && !e.altKey && e.code >= "Digit1" && e.code <= "Digit4") {
      const idx = Number(e.code.slice(-1)) - 1;
      if (idx >= 0 && idx < viewOrder.length) {
        e.preventDefault();
        activeView = viewOrder[idx];
      }
    }
  }}
/>

{#if showSearch}
  <SearchOverlay
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
{#if taskStore.error}
  <div style="background:var(--danger);color:white;padding:8px 12px;border-radius:6px;
    margin-bottom:10px;display:flex;justify-content:space-between;align-items:center;gap:12px;">
    <span>{taskStore.error}</span>
    <button onclick={() => taskStore.clearError()}
      style="background:transparent;border:none;color:white;padding:2px 6px;">✕</button>
  </div>
{/if}

{#if noteStore.error}
  <div style="background:var(--danger);color:white;padding:8px 12px;border-radius:6px;
    margin-bottom:10px;display:flex;justify-content:space-between;align-items:center;gap:12px;">
    <span>{noteStore.error}</span>
    <button onclick={() => noteStore.clearError()}
      style="background:transparent;border:none;color:white;padding:2px 6px;">✕</button>
  </div>
{/if}

<div style="display:flex;align-items:center;gap:8px;margin-bottom:12px;border-bottom:1px solid var(--border,#e5e7eb);padding-bottom:8px;">
  <button
    onclick={() => activeView = "tasks"}
    style="font-weight:{activeView === 'tasks' ? '600' : '400'};"
  >Задачи</button>
  <button
    onclick={() => activeView = "notes"}
    style="font-weight:{activeView === 'notes' ? '600' : '400'};"
  >Заметки</button>
  <button
    onclick={() => activeView = "settings"}
    style="font-weight:{activeView === 'settings' ? '600' : '400'};"
  >Настройки</button>
  <button
    onclick={() => activeView = "dashboard"}
    style="font-weight:{activeView === 'dashboard' ? '600' : '400'};"
  >Дашборд</button>
  <span style="flex:1;"></span>
  <button onclick={() => showSearch = true} title="Поиск (Ctrl+K)">Поиск</button>
</div>

{#if activeView === "tasks"}
  <Tasks />
{:else if activeView === "notes"}
  <Notes />
{:else if activeView === "settings"}
  <Settings />
{:else if activeView === "dashboard"}
  <Dashboard />
{/if}
{/if}
