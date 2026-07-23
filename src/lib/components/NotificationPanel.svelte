<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api/tauri";
  import Icon from "./Icon.svelte";
  import type { NotificationEntry } from "../types";

  let { onClose }: { onClose: () => void } = $props();

  let entries: NotificationEntry[] = $state([]);
  let loading = $state(true);

  const KIND_ICONS: Record<string, string> = {
    deadline: "flag",
    block: "calendar",
    digest: "sun",
    goal: "target",
    app_limit: "clock",
    pomodoro: "timer",
    overdue: "alert",
    missed_days: "alert",
    nudge: "coffee",
    activity_return: "bell",
  };

  function formatWhen(iso: string): string {
    const d = new Date(iso);
    const now = new Date();
    const sameDay = d.toDateString() === now.toDateString();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (sameDay) return time;
    return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
  }

  onMount(async () => {
    try {
      entries = await api.getNotificationLog();
    } finally {
      loading = false;
    }
    // Открытие панели считается прочтением всей текущей ленты.
    await api.markNotificationsRead().catch(() => {});
  });

  async function clearAll() {
    await api.clearNotificationLog().catch(() => {});
    entries = [];
  }
</script>

<div
  class="notif-overlay"
  role="presentation"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  onkeydown={(e) => { if (e.key === "Escape") onClose(); }}
>
  <div class="notif-panel card" role="dialog" aria-modal="true">
    <div class="notif-head">
      <span class="notif-title">Уведомления</span>
      {#if entries.length > 0}
        <button class="btn-sm" onclick={clearAll}>Очистить</button>
      {/if}
      <button class="btn-icon" title="Закрыть" onclick={onClose}>✕</button>
    </div>

    {#if loading}
      <div class="notif-empty muted">Загрузка…</div>
    {:else if entries.length === 0}
      <div class="notif-empty muted">Уведомлений пока не было</div>
    {:else}
      <ul class="notif-list">
        {#each entries as e (e.id)}
          <li class="notif-row">
            <span class="notif-icon"><Icon name={KIND_ICONS[e.kind] ?? "bell"} size={13} /></span>
            <div class="notif-body">
              <div class="notif-row-title">{e.title}</div>
              <div class="notif-row-text">{e.body}</div>
            </div>
            <span class="notif-when muted">{formatWhen(e.created_at)}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .notif-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
  }

  .notif-panel {
    position: fixed;
    left: 8px;
    bottom: 84px;
    width: 320px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.3);
  }

  .notif-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .notif-title {
    font-weight: 600;
    font-size: 13px;
    flex: 1;
  }

  .notif-empty {
    padding: 20px 12px;
    text-align: center;
    font-size: 12px;
  }

  .notif-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
  }

  .notif-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px;
    border-radius: var(--radius);
  }

  .notif-row:hover {
    background: var(--bg-hover);
  }

  .notif-icon {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--text-secondary);
  }

  .notif-body {
    flex: 1;
    min-width: 0;
  }

  .notif-row-title {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .notif-row-text {
    font-size: 11px;
    color: var(--text-secondary);
    white-space: pre-wrap;
  }

  .notif-when {
    flex-shrink: 0;
    font-size: 10px;
    white-space: nowrap;
  }
</style>
