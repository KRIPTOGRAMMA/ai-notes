<script lang="ts">
  import { taskStore } from "./lib/stores/tasks.svelte";
  import { noteStore } from "./lib/stores/notes.svelte";
  import { api } from "./lib/api/tauri";
  import Tasks from "./views/Tasks.svelte";
  import Notes from "./views/Notes.svelte";
  import "./app.css";

  type View = "tasks" | "notes";
  let activeView: View = $state("tasks");

  let isDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);

  if (typeof document !== "undefined") {
    document.documentElement.classList.toggle(
      "dark",
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  }

  function toggleTheme() {
    isDark = !isDark;
    document.documentElement.classList.toggle("dark", isDark);
  }

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
    if ((e.ctrlKey && e.code === "KeyK") || (e.ctrlKey && e.shiftKey && e.code === "KeyN")) {
      e.preventDefault();
      api.openQuickTask().catch(() => {});
    }
  }}
/>

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
  <span style="flex:1;"></span>
  <button onclick={toggleTheme}>{isDark ? "Светлая" : "Тёмная"}</button>
</div>

{#if activeView === "tasks"}
  <Tasks />
{:else if activeView === "notes"}
  <Notes />
{/if}
