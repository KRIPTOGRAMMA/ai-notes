<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";
  import type { ModelOption } from "../types";

  let options: ModelOption[] = $state([]);
  let selectedId: string = $state("");
  let customUrl = $state("");
  let usingCustomUrl = $state(false);
  let exists = $state(false);
  let sizeBytes = $state(0);
  let downloading = $state(false);
  let pct = $state(0);
  let error: string | null = $state(null);
  let unlisten: (() => void) | null = null;

  const mb = (b: number) => (b / 1024 / 1024).toFixed(1);
  const gb = (b: number) => (b / 1024 / 1024 / 1024).toFixed(1);

  const selectedUrl = $derived(
    usingCustomUrl ? customUrl : (options.find(o => o.id === selectedId)?.url ?? "")
  );

  async function refresh() {
    const s = await api.modelStatus();
    exists = s.exists;
    sizeBytes = s.size_bytes;
  }

  onMount(async () => {
    try {
      options = await api.listModelOptions();
      const recommended = options.find(o => o.recommended) ?? options[0];
      if (recommended) selectedId = recommended.id;
      await refresh();
      unlisten = await listen<{ pct: number }>("model-download-progress", ({ payload }) => {
        pct = payload.pct;
      });
    } catch (e) {
      error = String(e);
    }
  });

  onDestroy(() => unlisten?.());

  async function download() {
    if (!selectedUrl) return;
    error = null;
    downloading = true;
    pct = 0;
    try {
      await api.downloadModel(selectedUrl);
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      downloading = false;
    }
  }
</script>

<div class="model-picker">
  {#if exists}
    <div class="status ok">✓ Модель загружена ({mb(sizeBytes)} МБ)</div>
  {:else}
    <div class="status">Модель не найдена</div>
  {/if}

  <div class="option-list">
    {#each options as opt (opt.id)}
      <label class="option" class:active={!usingCustomUrl && selectedId === opt.id}>
        <input
          type="radio"
          name="model-option"
          checked={!usingCustomUrl && selectedId === opt.id}
          disabled={downloading}
          onchange={() => { usingCustomUrl = false; selectedId = opt.id; }}
        />
        <div class="option-body">
          <div class="option-title">
            {opt.name}
            {#if opt.recommended}<span class="chip-recommended">рекомендуется</span>{/if}
          </div>
          <div class="option-meta">~{gb(opt.size_bytes)} ГБ · от {opt.ram_gb} ГБ ОЗУ</div>
          <div class="option-desc">{opt.description}</div>
        </div>
      </label>
    {/each}

    <label class="option" class:active={usingCustomUrl}>
      <input
        type="radio"
        name="model-option"
        checked={usingCustomUrl}
        disabled={downloading}
        onchange={() => { usingCustomUrl = true; }}
      />
      <div class="option-body">
        <div class="option-title">Свой URL (GGUF)</div>
        <input
          type="text"
          bind:value={customUrl}
          disabled={downloading}
          placeholder="https://.../model.gguf"
          class="custom-url-input"
          onfocus={() => { usingCustomUrl = true; }}
        />
      </div>
    </label>
  </div>

  {#if downloading}
    <div class="progress-track">
      <div class="progress-fill" style="width:{pct}%;"></div>
    </div>
    <div class="progress-label">Загрузка… {pct}%</div>
  {:else}
    <button class="btn-primary btn-sm" onclick={download} disabled={!selectedUrl}>
      {exists ? "Перекачать" : "Скачать модель"}
    </button>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {/if}
</div>

<style>
  .model-picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .status {
    font-size: 13px;
    color: var(--text-secondary);
  }
  .status.ok {
    color: #16a34a;
  }
  .option-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .option {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .option.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .option input[type="radio"] {
    margin-top: 3px;
    flex-shrink: 0;
  }
  .option-body {
    flex: 1;
    min-width: 0;
  }
  .option-title {
    font-size: 13px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .chip-recommended {
    font-size: 10px;
    font-weight: 500;
    padding: 1px 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
  }
  .option-meta {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 2px;
  }
  .option-desc {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 3px;
  }
  .custom-url-input {
    display: block;
    width: 100%;
    margin-top: 6px;
    box-sizing: border-box;
    font-size: 12px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-primary);
    color: var(--text-primary);
  }
  .progress-track {
    background: var(--bg-secondary);
    border-radius: 6px;
    height: 10px;
    overflow: hidden;
  }
  .progress-fill {
    background: var(--accent);
    height: 100%;
    transition: width 0.2s;
  }
  .progress-label {
    font-size: 12px;
    color: var(--text-secondary);
  }
  .error {
    font-size: 12px;
    color: #dc2626;
  }
</style>
