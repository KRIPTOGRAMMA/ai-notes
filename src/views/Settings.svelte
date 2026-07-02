<script lang="ts">
  import { onMount } from "svelte";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api/tauri";
  import type { AppSettings } from "../lib/types";

  let settings: AppSettings = $state({
    ai_provider: "local",
    openai_key: "",
    openai_model: "gpt-4o-mini",
    anthropic_key: "",
    anthropic_model: "claude-haiku-4-5-20251001",
  });

  let saving = $state(false);
  let saved = $state(false);
  let error: string | null = $state(null);

  onMount(async () => {
    try {
      settings = await api.getSettings();
    } catch (e) {
      error = String(e);
    }
  });

  async function save() {
    saving = true;
    error = null;
    try {
      await api.saveSettings(settings);
      saved = true;
      setTimeout(() => saved = false, 2000);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  let backupMsg: string | null = $state(null);

  async function exportData() {
    backupMsg = null;
    error = null;
    try {
      const path = await saveDialog({
        defaultPath: "ai-notes-backup.zip",
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (!path) return;
      await api.exportData(path);
      backupMsg = "Экспорт завершён ✓";
    } catch (e) {
      error = String(e);
    }
  }

  async function importData() {
    backupMsg = null;
    error = null;
    if (!confirm("Импорт заменит все текущие данные. Продолжить?")) return;
    try {
      const path = await openDialog({
        multiple: false,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (!path) return;
      await api.importData(path as string);
      backupMsg = "Импорт завершён ✓ Приложение перезапускается...";
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div style="max-width:520px;padding:4px;">
  <h2 style="margin-top:0;">Настройки</h2>

  {#if error}
    <div style="background:#fee2e2;color:#dc2626;padding:8px 12px;border-radius:6px;margin-bottom:12px;">{error}</div>
  {/if}

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">ИИ-провайдер</h3>

    <div style="display:flex;flex-direction:column;gap:8px;">
      {#each [["local","Локальная модель (llamafile)"],["openai","OpenAI"],["anthropic","Anthropic"]] as [val, label]}
        <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="radio" name="provider" value={val} bind:group={settings.ai_provider} />
          {label}
        </label>
      {/each}
    </div>
  </section>

  {#if settings.ai_provider === "openai"}
    <section style="margin-bottom:24px;">
      <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">OpenAI</h3>
      <div style="display:flex;flex-direction:column;gap:8px;">
        <label style="font-size:13px;">
          API Key
          <input
            type="password"
            bind:value={settings.openai_key}
            placeholder="sk-..."
            style="display:block;width:100%;margin-top:4px;box-sizing:border-box;"
          />
        </label>
        <label style="font-size:13px;">
          Модель
          <select bind:value={settings.openai_model} style="display:block;width:100%;margin-top:4px;">
            <option value="gpt-4o-mini">gpt-4o-mini (быстрый, дешёвый)</option>
            <option value="gpt-4o">gpt-4o</option>
            <option value="gpt-4-turbo">gpt-4-turbo</option>
          </select>
        </label>
      </div>
    </section>
  {/if}

  {#if settings.ai_provider === "anthropic"}
    <section style="margin-bottom:24px;">
      <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Anthropic</h3>
      <div style="display:flex;flex-direction:column;gap:8px;">
        <label style="font-size:13px;">
          API Key
          <input
            type="password"
            bind:value={settings.anthropic_key}
            placeholder="sk-ant-..."
            style="display:block;width:100%;margin-top:4px;box-sizing:border-box;"
          />
        </label>
        <label style="font-size:13px;">
          Модель
          <select bind:value={settings.anthropic_model} style="display:block;width:100%;margin-top:4px;">
            <option value="claude-haiku-4-5-20251001">claude-haiku-4-5 (быстрый, дешёвый)</option>
            <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
          </select>
        </label>
      </div>
    </section>
  {/if}

  {#if settings.ai_provider === "local"}
    <section style="margin-bottom:24px;">
      <p style="font-size:13px;color:var(--text-secondary,#6b7280);margin:0;">
        Модель должна находиться по пути:<br/>
        <code>~/.local/share/ai-notes/models/model.gguf</code>
      </p>
    </section>
  {/if}

  <button onclick={save} disabled={saving}>
    {saving ? "Сохранение..." : saved ? "Сохранено ✓" : "Сохранить"}
  </button>

  <section style="margin-top:32px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Данные</h3>
    <div style="display:flex;gap:8px;align-items:center;">
      <button onclick={exportData}>Экспорт (ZIP)</button>
      <button onclick={importData}>Импорт (ZIP)</button>
      {#if backupMsg}
        <span style="font-size:12px;color:var(--text-secondary,#6b7280);">{backupMsg}</span>
      {/if}
    </div>
  </section>
</div>
