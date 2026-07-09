<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../api/tauri";

  let url = $state("");
  let exists = $state(false);
  let sizeBytes = $state(0);
  let downloading = $state(false);
  let pct = $state(0);
  let error: string | null = $state(null);
  let unlisten: (() => void) | null = null;

  const mb = (b: number) => (b / 1024 / 1024).toFixed(1);

  async function refresh() {
    const s = await api.modelStatus();
    exists = s.exists;
    sizeBytes = s.size_bytes;
  }

  onMount(async () => {
    try {
      url = await api.defaultModelUrl();
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
    error = null;
    downloading = true;
    pct = 0;
    try {
      await api.downloadModel(url);
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      downloading = false;
    }
  }
</script>

<div style="display:flex;flex-direction:column;gap:8px;">
  {#if exists}
    <div style="font-size:13px;color:#16a34a;">✓ Модель загружена ({mb(sizeBytes)} МБ)</div>
  {:else}
    <div style="font-size:13px;color:var(--text-secondary,#6b7280);">Модель не найдена</div>
  {/if}

  <label style="font-size:13px;">
    URL модели (GGUF)
    <input
      type="text"
      bind:value={url}
      disabled={downloading}
      style="display:block;width:100%;margin-top:4px;box-sizing:border-box;font-size:12px;"
    />
  </label>

  {#if downloading}
    <div style="background:#e5e7eb;border-radius:6px;height:10px;overflow:hidden;">
      <div style="background:#2563eb;height:100%;width:{pct}%;transition:width .2s;"></div>
    </div>
    <div style="font-size:12px;color:var(--text-secondary,#6b7280);">Загрузка… {pct}%</div>
  {:else}
    <button onclick={download} disabled={!url}>
      {exists ? "Перекачать" : "Скачать модель"}
    </button>
  {/if}

  {#if error}
    <div style="font-size:12px;color:#dc2626;">{error}</div>
  {/if}
</div>
