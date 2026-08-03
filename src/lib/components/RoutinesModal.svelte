<script lang="ts">
  import { routineStore } from "../stores/routines.svelte";
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";

  // `t` is taken here by a local variable, so the translation helper is imported as `tr`.
  import { t as tr, tErr as trErr } from "../i18n.svelte";
  import { hhmmFromMins } from "../datetime";
  let { onClose }: { onClose: () => void } = $props();

  const DAYS = $derived([tr("Пн"), tr("Вт"), tr("Ср"), tr("Чт"), tr("Пт"), tr("Сб"), tr("Вс")]);

  let editingId = $state<string | null | undefined>(undefined);
  let editTitle = $state("");
  let editDays = $state<boolean[]>([false, false, false, false, false, false, false]);
  let editStart = $state("08:00");
  let editDuration = $state("60");
  let editActive = $state(true);

  function openNew() {
    editingId = null;
    editTitle = "";
    editDays = [false, false, false, false, false, false, false];
    editStart = "08:00";
    editDuration = "60";
    editActive = true;
  }

  function openEdit(r: { id: string; title: string; days_mask: number; start_mins: number; duration_mins: number; active: boolean }) {
    editingId = r.id;
    editTitle = r.title;
    editDays = DAYS.map((_, i) => (r.days_mask & (1 << i)) !== 0);
    editStart = hhmmFromMins(r.start_mins);
    editDuration = String(r.duration_mins);
    editActive = r.active;
  }

  function toMask(days: boolean[]): number {
    return days.reduce((acc, d, i) => acc | (d ? 1 << i : 0), 0);
  }

  function toMins(t: string): number {
    const [h, m] = t.split(":").map(Number);
    return h * 60 + (m || 0);
  }

  async function save() {
    if (!editTitle.trim()) return;
    const mask = toMask(editDays);
    if (mask === 0) return;
    const start = toMins(editStart);
    const dur = Math.max(15, parseInt(editDuration) || 60);
    if (editingId) {
      await routineStore.update(editingId, {
        title: editTitle.trim(),
        days_mask: mask,
        start_mins: start,
        duration_mins: dur,
      });
    } else {
      await routineStore.create(editTitle.trim(), mask, start, dur);
    }
    editingId = undefined;
  }

  onMount(() => {
    routineStore.load();
  });
</script>

<div class="backdrop" role="presentation" onclick={onClose} onkeydown={(e) => e.key === "Escape" && onClose()}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog card" role="dialog" onclick={(e) => e.stopPropagation()}>
    <h3 class="dialog-title">{tr("Рутины")}</h3>

    <!-- Routines are only created and edited here, so this is the one place their
         failures can reach the user. -->
    {#if routineStore.error}
      <div class="alert routine-error">
        <span>{trErr(routineStore.error)}</span>
        <button class="btn-icon" onclick={() => routineStore.clearError()}>✕</button>
      </div>
    {/if}

    <div class="list">
      {#each routineStore.routines as r (r.id)}
        <div class="row">
          <div class="row-info">
            <span class="row-title" class:inactive={!r.active}>{r.title}</span>
            <span class="row-meta">
              {hhmmFromMins(r.start_mins)} – {hhmmFromMins(r.start_mins + r.duration_mins)}
              · {DAYS.filter((_, i) => r.days_mask & (1 << i)).join(" ")}
            </span>
          </div>
          <div class="row-actions">
            <button class="btn-icon" onclick={() => routineStore.update(r.id, { active: !r.active })} title={r.active ? tr("Выключить") : tr("Включить")}>
              {r.active ? "✓" : "○"}
            </button>
            <button class="btn-icon" onclick={() => openEdit(r)} title={tr("Редактировать")}><Icon name="pencil" /></button>
            <button class="btn-icon" onclick={() => routineStore.remove(r.id)} title={tr("Удалить")}>✕</button>
          </div>
        </div>
      {/each}
    </div>

    {#if editingId !== undefined}
      <div class="edit-form">
        <input bind:value={editTitle} placeholder={tr("Название рутины")} />
        <div class="day-picker">
          {#each DAYS as d, i}
            <label class="day-chip">
              <input type="checkbox" bind:checked={editDays[i]} />
              <span>{d}</span>
            </label>
          {/each}
        </div>
        <div class="time-row">
          <label>{tr("Начало")} <input type="time" bind:value={editStart} /></label>
          <label>{tr("Длительность (мин)")} <input type="number" bind:value={editDuration} min="15" style="width:70px;" /></label>
        </div>
        <div class="actions">
          <button class="btn-ghost" onclick={() => editingId = undefined}>{tr("Отмена")}</button>
          <button class="btn-primary" onclick={save}>{editingId ? tr("Сохранить") : tr("Добавить")}</button>
        </div>
      </div>
    {:else}
      <button class="btn-sm" style="margin-top:8px;" onclick={openNew}>{tr("+ Добавить рутину")}</button>
    {/if}

    <button class="btn-ghost" style="margin-top:8px;align-self:flex-end;" onclick={onClose}>{tr("Закрыть")}</button>
  </div>
</div>

<style>
  /* .alert is a plain block globally; the dismiss button needs it to be a row. */
  .routine-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 16px;
  }

  .dialog {
    width: 100%;
    max-width: 460px;
    max-height: 90vh;
    overflow-y: auto;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .dialog-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 300px;
    overflow-y: auto;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .row-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .row-title {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-title.inactive {
    opacity: 0.5;
  }

  .row-meta {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .row-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
  }

  .day-picker {
    display: flex;
    gap: 4px;
  }

  .day-chip {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 12px;
  }

  .day-chip input {
    margin: 0;
  }

  .time-row {
    display: flex;
    gap: 12px;
  }

  .time-row label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }

  .actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
</style>
