<script lang="ts">
  import { onMount } from "svelte";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api/tauri";
  import type { AppSettings } from "../lib/types";
  import { applyTheme } from "../lib/theme";
  import ModelDownloader from "../lib/components/ModelDownloader.svelte";

  const THEME_PRESETS: { name: string; accent: string }[] = [
    { name: "Indigo", accent: "#6366f1" },
    { name: "Emerald", accent: "#10b981" },
    { name: "Rose", accent: "#f43f5e" },
    { name: "Slate", accent: "#64748b" },
  ];

  // Применяем тему сразу при любом изменении — живое превью без нажатия «Сохранить».
  function previewTheme() {
    applyTheme(settings.theme_mode, settings);
  }

  function applyPreset(accent: string) {
    settings.color_accent = accent;
    previewTheme();
  }

  function resetColors() {
    settings.color_accent = "";
    settings.color_bg = "";
    settings.color_text = "";
    settings.color_border = "";
    previewTheme();
  }

  let settings: AppSettings = $state({
    ai_provider: "local",
    openai_key: "",
    openai_model: "gpt-4o-mini",
    anthropic_key: "",
    anthropic_model: "claude-haiku-4-5-20251001",
    idle_threshold_secs: 300,
    log_interval_secs: 60,
    work_mode: "Light",
    onboarding_complete: true,
    deadline_warn_hours: 24,
    deadline_warn_minutes: 60,
    idle_notify_min_mins: 10,
    pomodoro_work_mins: 25,
    pomodoro_break_mins: 5,
    nudge_after_mins: 90,
    theme_mode: "system",
    color_accent: "",
    color_bg: "",
    color_text: "",
    color_border: "",
    quiet_until: "",
    context_notifications: true,
    ai_fallback: false,
    openai_in_keyring: false,
    anthropic_in_keyring: false,
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
      applyTheme(settings.theme_mode, settings);
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

<div class="settings">
  <h2 class="page-title" style="margin-bottom:14px;">Настройки</h2>

  {#if error}
    <div class="alert">{error}</div>
  {/if}

  <section class="card panel">
    <h3 class="section-title">Внешний вид</h3>

    <div class="radio-row">
      {#each [["light","Светлая"],["dark","Тёмная"],["system","Системная"]] as [val, label]}
        <label class="check">
          <input type="radio" name="theme_mode" value={val} bind:group={settings.theme_mode} onchange={previewTheme} />
          {label}
        </label>
      {/each}
    </div>

    <div class="sub-label">Пресеты акцента</div>
    <div class="preset-row">
      {#each THEME_PRESETS as p}
        <button type="button" class="btn-sm" onclick={() => applyPreset(p.accent)}>
          <span class="swatch" style="background:{p.accent};"></span>
          {p.name}
        </button>
      {/each}
    </div>

    <div class="color-grid">
      {#each [["color_accent","Акцент"],["color_bg","Фон"],["color_text","Текст"],["color_border","Границы"]] as [key, label]}
        <label class="check">
          <input type="color"
            value={(settings as any)[key] || "#6366f1"}
            oninput={(e) => { (settings as any)[key] = e.currentTarget.value; previewTheme(); }}
            class="color-input" />
          {label}
        </label>
      {/each}
    </div>

    <button type="button" class="btn-sm" style="margin-top:10px;" onclick={resetColors}>Сбросить к дефолту</button>
  </section>

  <section class="card panel">
    <h3 class="section-title">ИИ-провайдер</h3>

    <div class="stack">
      {#each [["none","Без ИИ (функции отключены)"],["local","Локальная модель (llamafile)"],["openai","OpenAI"],["anthropic","Anthropic"]] as [val, label]}
        <label class="check">
          <input type="radio" name="provider" value={val} bind:group={settings.ai_provider} />
          {label}
        </label>
      {/each}
    </div>

    {#if settings.ai_provider !== "none"}
      <label class="check" style="margin-top:10px;">
        <input type="checkbox" bind:checked={settings.ai_fallback} />
        Автопереключение: при ошибке или недоступности пробовать других доступных провайдеров
      </label>
    {/if}

    {#if settings.ai_provider === "openai"}
      <div class="stack" style="margin-top:12px;">
        <label class="field">
          <span class="label">API Key
            {#if settings.openai_key}
              {#if settings.openai_in_keyring}
                <span class="key-ok">🔐 keyring</span>
              {:else}
                <span class="key-warn">⚠ БД (keyring недоступен)</span>
              {/if}
            {/if}
          </span>
          <input type="password" bind:value={settings.openai_key} placeholder="sk-..." />
        </label>
        <label class="field">
          <span class="label">Модель</span>
          <select bind:value={settings.openai_model}>
            <option value="gpt-4o-mini">gpt-4o-mini (быстрый, дешёвый)</option>
            <option value="gpt-4o">gpt-4o</option>
            <option value="gpt-4-turbo">gpt-4-turbo</option>
          </select>
        </label>
      </div>
    {/if}

    {#if settings.ai_provider === "anthropic"}
      <div class="stack" style="margin-top:12px;">
        <label class="field">
          <span class="label">API Key
            {#if settings.anthropic_key}
              {#if settings.anthropic_in_keyring}
                <span class="key-ok">🔐 keyring</span>
              {:else}
                <span class="key-warn">⚠ БД (keyring недоступен)</span>
              {/if}
            {/if}
          </span>
          <input type="password" bind:value={settings.anthropic_key} placeholder="sk-ant-..." />
        </label>
        <label class="field">
          <span class="label">Модель</span>
          <select bind:value={settings.anthropic_model}>
            <option value="claude-haiku-4-5-20251001">claude-haiku-4-5 (быстрый, дешёвый)</option>
            <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
          </select>
        </label>
      </div>
    {/if}

    {#if settings.ai_provider === "local"}
      <div style="margin-top:12px;">
        <p class="muted" style="font-size:12px;margin:0 0 10px 0;">
          Локальная модель хранится в <code>~/.local/share/ai-notes/models/model.gguf</code>
        </p>
        <ModelDownloader />
      </div>
    {/if}
  </section>

  <section class="card panel">
    <h3 class="section-title">Режим работы</h3>
    <select bind:value={settings.work_mode} style="width:100%;">
      <option value="Light">Light — обычный режим</option>
      <option value="Focus">Focus — без уведомлений</option>
      <option value="Study">Study — помодоро-сессии (25/5)</option>
    </select>
    <p class="hint">Применяется сразу после сохранения.</p>

    {#if settings.work_mode === "Study"}
      <div class="pair" style="margin-top:10px;">
        <label class="field">
          <span class="label">Рабочий блок (мин)</span>
          <input type="number" min="1" max="120" bind:value={settings.pomodoro_work_mins} />
        </label>
        <label class="field">
          <span class="label">Перерыв (мин)</span>
          <input type="number" min="1" max="60" bind:value={settings.pomodoro_break_mins} />
        </label>
      </div>
      <p class="hint">Применяется при следующем входе в режим Study.</p>
    {/if}
  </section>

  <section class="card panel">
    <h3 class="section-title">Мониторинг</h3>
    <div class="pair">
      <label class="field">
        <span class="label">Порог простоя (сек, мин. 60)</span>
        <input type="number" min="60" bind:value={settings.idle_threshold_secs} />
      </label>
      <label class="field">
        <span class="label">Интервал логирования (сек, 10–600)</span>
        <input type="number" min="10" max="600" bind:value={settings.log_interval_secs} />
      </label>
    </div>
    <p class="hint">Применяется после перезапуска приложения.</p>
  </section>

  <section class="card panel">
    <h3 class="section-title">Уведомления</h3>
    <div class="pair">
      <label class="field">
        <span class="label">Первое предупреждение (часов до дедлайна)</span>
        <input type="number" min="1" bind:value={settings.deadline_warn_hours} />
      </label>
      <label class="field">
        <span class="label">Второе предупреждение (минут до дедлайна)</span>
        <input type="number" min="1" max="1440" bind:value={settings.deadline_warn_minutes} />
      </label>
      <label class="field">
        <span class="label">Возврат после простоя (мин, мин. 1)</span>
        <input type="number" min="1" bind:value={settings.idle_notify_min_mins} />
      </label>
      <label class="field">
        <span class="label">Перерыв после N минут работы (0 — выкл)</span>
        <input type="number" min="0" bind:value={settings.nudge_after_mins} />
      </label>
    </div>
    <label class="check" style="margin-top:10px;">
      <input type="checkbox" bind:checked={settings.context_notifications} />
      Контекстные уведомления (накопились просрочки, возврат к задаче «в работе»)
    </label>
    <p class="hint">
      Пауза всех уведомлений — в меню трея: «Пауза уведомлений» (30 мин / 1 ч / 2 ч / бессрочно).
    </p>
  </section>

  <section class="card panel">
    <h3 class="section-title">Данные</h3>
    <div class="preset-row">
      <button class="btn-sm" onclick={exportData}>Экспорт (ZIP)</button>
      <button class="btn-sm" onclick={importData}>Импорт (ZIP)</button>
      {#if backupMsg}
        <span class="muted" style="font-size:12px;">{backupMsg}</span>
      {/if}
    </div>
  </section>

  <button class="btn-primary" onclick={save} disabled={saving}>
    {saving ? "Сохранение..." : saved ? "Сохранено ✓" : "Сохранить"}
  </button>
</div>

<style>
  .settings {
    max-width: 560px;
    padding-bottom: 24px;
  }

  .panel {
    padding: 14px 16px;
    margin-bottom: 12px;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 14px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-size: 13px;
  }

  .radio-row {
    display: flex;
    gap: 16px;
    margin-bottom: 12px;
  }

  .sub-label {
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 6px;
  }

  .preset-row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }

  .swatch {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    display: inline-block;
    margin-right: 4px;
    vertical-align: -1px;
  }

  .color-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px 16px;
    max-width: 380px;
    margin-top: 12px;
  }

  .color-input {
    width: 34px;
    height: 26px;
    padding: 0;
    border-radius: 4px;
  }

  .hint {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 8px 0 0 0;
  }

  .key-ok {
    font-size: 11px;
    color: var(--success);
    margin-left: 6px;
    text-transform: none;
    letter-spacing: 0;
  }

  .key-warn {
    font-size: 11px;
    color: var(--cat-home);
    margin-left: 6px;
    text-transform: none;
    letter-spacing: 0;
  }

  code {
    background: var(--bg-secondary);
    padding: 1px 4px;
    border-radius: 4px;
    font-size: 0.95em;
  }
</style>
