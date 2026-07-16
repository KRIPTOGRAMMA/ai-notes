<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../api/tauri";

  // Опрос раз в секунду — состояние живёт в БД (settings), пишется циклом
  // на бэкенде при каждой смене фазы; здесь просто отражаем его.
  let phase = $state<"work" | "break" | "paused" | "off">("off");
  let until: Date | null = $state(null);
  let now = $state(new Date());

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let tickTimer: ReturnType<typeof setInterval> | null = null;

  async function poll() {
    try {
      const s = await api.getPomodoroState();
      phase = (s.phase as typeof phase) ?? "off";
      until = s.until ? new Date(s.until) : null;
    } catch {
      // ИИ-провайдер тут ни при чём, а вот трекер могло не поднять —
      // тихо оставляем предыдущее состояние
    }
  }

  onMount(() => {
    poll();
    pollTimer = setInterval(poll, 3000);
    tickTimer = setInterval(() => { now = new Date(); }, 1000);
  });
  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (tickTimer) clearInterval(tickTimer);
  });

  const remainingLabel = $derived.by(() => {
    if (!until) return "";
    const secs = Math.max(0, Math.round((until.getTime() - now.getTime()) / 1000));
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  });

  const phaseLabel = $derived(
    phase === "work" ? "🍅 Фокус" : phase === "break" ? "☕ Перерыв" : phase === "paused" ? "🍅 Пауза" : ""
  );

  async function togglePause() {
    await api.pomodoroTogglePause();
    await poll();
  }
  async function skip() {
    await api.pomodoroSkip();
    await poll();
  }
</script>

{#if phase !== "off"}
  <div class="pomo card">
    <span class="pomo-label">{phaseLabel}</span>
    {#if phase !== "paused"}
      <span class="pomo-time">{remainingLabel}</span>
    {/if}
    <div class="pomo-actions">
      <button class="btn-icon" title={phase === "paused" ? "Продолжить" : "Пауза"} onclick={togglePause}>
        {phase === "paused" ? "▶" : "⏸"}
      </button>
      <button class="btn-icon" title="Пропустить фазу" onclick={skip}>⏭</button>
    </div>
  </div>
{/if}

<style>
  .pomo {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 8px 6px;
    margin: 0 8px 8px;
    font-size: 12px;
  }

  .pomo-label {
    font-weight: 600;
  }

  .pomo-time {
    font-variant-numeric: tabular-nums;
    font-size: 18px;
    color: var(--accent);
  }

  .pomo-actions {
    display: flex;
    gap: 4px;
    margin-top: 2px;
  }
</style>
