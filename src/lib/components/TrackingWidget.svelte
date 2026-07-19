<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../api/tauri";
  import type { ActiveSession } from "../types";

  let session: ActiveSession | null = $state(null);
  let now = $state(new Date());

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let tickTimer: ReturnType<typeof setInterval> | null = null;

  async function poll() {
    try {
      session = await api.getActiveSession();
    } catch {}
  }

  function stop() {
    api.stopTaskTracking().then(() => poll());
  }

  const elapsedLabel = $derived.by(() => {
    if (!session) return "";
    const started = new Date(session.started_at);
    const secs = Math.max(0, Math.round((now.getTime() - started.getTime()) / 1000));
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
    return `${m}:${String(s).padStart(2, "0")}`;
  });

  onMount(() => {
    poll();
    pollTimer = setInterval(poll, 3000);
    tickTimer = setInterval(() => { now = new Date(); }, 1000);
  });
  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (tickTimer) clearInterval(tickTimer);
  });
</script>

{#if session}
  <div class="track card">
    <span class="track-label">▶ {session.title}</span>
    <span class="track-time">{elapsedLabel}</span>
    <div class="track-actions">
      <button class="btn-icon" title="Остановить трекинг" onclick={stop}>■</button>
    </div>
  </div>
{/if}

<style>
  .track {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 8px 6px;
    margin: 0 8px 8px;
    font-size: 12px;
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-primary));
    border-color: var(--accent);
  }

  .track-label {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .track-time {
    font-variant-numeric: tabular-nums;
    font-size: 18px;
    color: var(--accent);
  }

  .track-actions {
    display: flex;
    gap: 4px;
    margin-top: 2px;
  }
</style>
