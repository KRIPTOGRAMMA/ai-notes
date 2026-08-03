<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../api/tauri";
  import Icon from "./Icon.svelte";

  import { t } from "../i18n.svelte";
  import { duration } from "../datetime";
  // Polled once a second: the state lives in the DB (settings) and is written by a
  // loop on the backend at every phase change; here we merely reflect it.
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
      // The AI provider has nothing to do with it, but the tracker may have failed to
      // start — we quietly keep the previous state
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
    return duration(secs, true);
  });

  // The phase icon is an SVG: timer for work and pauses, coffee for a break
  const phaseLabel = $derived(
    phase === "work" ? t("Фокус") : phase === "break" ? t("Перерыв") : phase === "paused" ? t("Пауза") : ""
  );
  const phaseIcon = $derived(phase === "break" ? "coffee" : "timer");

  async function togglePause() {
    await api.pomodoroTogglePause();
    await poll();
  }
  async function skip() {
    await api.pomodoroSkip();
    await poll();
  }
  async function start() {
    await api.pomodoroStart();
    await poll();
  }
  async function stop() {
    await api.pomodoroStop();
    await poll();
  }
</script>

{#if phase === "off"}
  <div class="pomo card">
    <button class="btn-icon" title={t("Начать помидор")} onclick={start}><Icon name="play" /> <Icon name="timer" /></button>
  </div>
{:else}
  <div class="pomo card">
    <span class="pomo-label"><Icon name={phaseIcon} /> {phaseLabel}</span>
    {#if phase !== "paused"}
      <span class="pomo-time">{remainingLabel}</span>
    {/if}
    <div class="pomo-actions">
      <button class="btn-icon" title={phase === "paused" ? t("Продолжить") : t("Пауза")} onclick={togglePause}>
        {#if phase === "paused"}<Icon name="play" />{:else}<Icon name="pause" />{/if}
      </button>
      <button class="btn-icon" title={t("Пропустить фазу")} onclick={skip}><Icon name="skip" /></button>
      <button class="btn-icon" title={t("Остановить")} onclick={stop}><Icon name="stop" /></button>
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
