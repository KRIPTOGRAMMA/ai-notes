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

<div style="max-width:520px;padding:4px;">
  <h2 style="margin-top:0;">Настройки</h2>

  {#if error}
    <div style="background:#fee2e2;color:#dc2626;padding:8px 12px;border-radius:6px;margin-bottom:12px;">{error}</div>
  {/if}

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Внешний вид</h3>

    <div style="display:flex;gap:16px;margin-bottom:12px;">
      {#each [["light","Светлая"],["dark","Тёмная"],["system","Системная"]] as [val, label]}
        <label style="display:flex;align-items:center;gap:6px;cursor:pointer;">
          <input type="radio" name="theme_mode" value={val} bind:group={settings.theme_mode} onchange={previewTheme} />
          {label}
        </label>
      {/each}
    </div>

    <div style="margin-bottom:10px;">
      <div style="font-size:12px;color:var(--text-secondary,#6b7280);margin-bottom:6px;">Пресеты акцента</div>
      <div style="display:flex;gap:8px;flex-wrap:wrap;">
        {#each THEME_PRESETS as p}
          <button type="button" onclick={() => applyPreset(p.accent)}
            style="display:flex;align-items:center;gap:6px;">
            <span style="width:12px;height:12px;border-radius:50%;background:{p.accent};display:inline-block;"></span>
            {p.name}
          </button>
        {/each}
      </div>
    </div>

    <div style="display:grid;grid-template-columns:repeat(2,1fr);gap:8px 16px;max-width:420px;">
      {#each [["color_accent","Акцент"],["color_bg","Фон"],["color_text","Текст"],["color_border","Границы"]] as [key, label]}
        <label style="display:flex;align-items:center;gap:8px;font-size:13px;">
          <input type="color"
            value={(settings as any)[key] || "#6366f1"}
            oninput={(e) => { (settings as any)[key] = e.currentTarget.value; previewTheme(); }}
            style="width:34px;height:26px;padding:0;border-radius:4px;" />
          {label}
        </label>
      {/each}
    </div>

    <button type="button" onclick={resetColors} style="margin-top:10px;">Сбросить к дефолту</button>
  </section>

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">ИИ-провайдер</h3>

    <div style="display:flex;flex-direction:column;gap:8px;">
      {#each [["none","Без ИИ (функции отключены)"],["local","Локальная модель (llamafile)"],["openai","OpenAI"],["anthropic","Anthropic"]] as [val, label]}
        <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="radio" name="provider" value={val} bind:group={settings.ai_provider} />
          {label}
        </label>
      {/each}
    </div>

    {#if settings.ai_provider !== "none"}
      <label style="display:flex;align-items:center;gap:8px;cursor:pointer;font-size:13px;margin-top:10px;">
        <input type="checkbox" bind:checked={settings.ai_fallback} />
        Автопереключение: при ошибке или недоступности пробовать других доступных провайдеров
      </label>
    {/if}
  </section>

  {#if settings.ai_provider === "openai"}
    <section style="margin-bottom:24px;">
      <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">OpenAI</h3>
      <div style="display:flex;flex-direction:column;gap:8px;">
        <label style="font-size:13px;">
          API Key
          {#if settings.openai_key}
            {#if settings.openai_in_keyring}
              <span style="font-size:11px;color:#16a34a;margin-left:6px;">🔐 keyring</span>
            {:else}
              <span style="font-size:11px;color:#d97706;margin-left:6px;">⚠ БД (keyring недоступен)</span>
            {/if}
          {/if}
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
          {#if settings.anthropic_key}
            {#if settings.anthropic_in_keyring}
              <span style="font-size:11px;color:#16a34a;margin-left:6px;">🔐 keyring</span>
            {:else}
              <span style="font-size:11px;color:#d97706;margin-left:6px;">⚠ БД (keyring недоступен)</span>
            {/if}
          {/if}
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
      <p style="font-size:13px;color:var(--text-secondary,#6b7280);margin:0 0 10px 0;">
        Локальная модель хранится в<br/>
        <code>~/.local/share/ai-notes/models/model.gguf</code>
      </p>
      <ModelDownloader />
    </section>
  {/if}

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Режим работы</h3>
    <label style="font-size:13px;">
      <select bind:value={settings.work_mode} style="display:block;width:100%;margin-top:4px;">
        <option value="Light">Light — обычный режим</option>
        <option value="Focus">Focus — без уведомлений</option>
        <option value="Study">Study — помодоро-сессии (25/5)</option>
      </select>
    </label>
    <p style="font-size:12px;color:var(--text-secondary,#6b7280);margin:6px 0 0 0;">
      Применяется сразу после сохранения.
    </p>
  </section>

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Мониторинг</h3>
    <div style="display:flex;flex-direction:column;gap:8px;">
      <label style="font-size:13px;">
        Порог простоя (секунды, мин. 60)
        <input
          type="number"
          min="60"
          bind:value={settings.idle_threshold_secs}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;"
        />
      </label>
      <label style="font-size:13px;">
        Интервал логирования (секунды, 10–600)
        <input
          type="number"
          min="10"
          max="600"
          bind:value={settings.log_interval_secs}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;"
        />
      </label>
      <p style="font-size:12px;color:var(--text-secondary,#6b7280);margin:0;">
        Применяется после перезапуска приложения.
      </p>
    </div>
  </section>

  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Уведомления о дедлайнах</h3>
    <div style="display:flex;flex-direction:column;gap:8px;">
      <label style="font-size:13px;">
        Первое предупреждение (часов до дедлайна)
        <input type="number" min="1" bind:value={settings.deadline_warn_hours}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
      <label style="font-size:13px;">
        Второе предупреждение (минут до дедлайна)
        <input type="number" min="1" max="1440" bind:value={settings.deadline_warn_minutes}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
      <label style="font-size:13px;">
        Уведомление о возвращении после простоя (минут, мин. 1)
        <input type="number" min="1" bind:value={settings.idle_notify_min_mins}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
      <label style="font-size:13px;">
        Напоминание о перерыве после N минут непрерывной работы (0 — выкл, только режим Light)
        <input type="number" min="0" bind:value={settings.nudge_after_mins}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
      <label style="display:flex;align-items:center;gap:8px;cursor:pointer;font-size:13px;">
        <input type="checkbox" bind:checked={settings.context_notifications} />
        Контекстные уведомления (накопились просрочки, возврат к задаче «в работе»)
      </label>
      <p style="font-size:12px;color:var(--text-secondary,#6b7280);margin:0;">
        Пауза всех уведомлений — в меню трея: «Пауза уведомлений» (30 мин / 1 ч / 2 ч / бессрочно).
      </p>
    </div>
  </section>

  {#if settings.work_mode === "Study"}
  <section style="margin-bottom:24px;">
    <h3 style="margin:0 0 10px 0;font-size:14px;text-transform:uppercase;color:var(--text-secondary,#6b7280);letter-spacing:.05em;">Помодоро</h3>
    <div style="display:flex;flex-direction:column;gap:8px;">
      <label style="font-size:13px;">
        Рабочий блок (минуты)
        <input type="number" min="1" max="120" bind:value={settings.pomodoro_work_mins}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
      <label style="font-size:13px;">
        Перерыв (минуты)
        <input type="number" min="1" max="60" bind:value={settings.pomodoro_break_mins}
          style="display:block;width:100%;margin-top:4px;box-sizing:border-box;" />
      </label>
    </div>
    <p style="font-size:12px;color:var(--text-secondary,#6b7280);margin:6px 0 0 0;">
      Применяется при следующем входе в режим Study.
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
