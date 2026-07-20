<script lang="ts">
  import { onMount } from "svelte";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api/tauri";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import type { AppSettings, AppCategoryRule, AppLimit } from "../lib/types";
  import { applyTheme } from "../lib/theme";
  import ModelDownloader from "../lib/components/ModelDownloader.svelte";
  import Icon from "../lib/components/Icon.svelte";

  const PROVIDERS: { value: AppSettings["ai_provider"]; label: string }[] = [
    { value: "none", label: "Без ИИ (функции отключены)" },
    { value: "local", label: "Локальная модель (llamafile)" },
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic" },
  ];

  // Каждый пресет задаёт пару акцентов (основной + дополнительный, градиент
  // на .btn-primary) одной кнопкой; «Свой» — ручной выбор ниже остаётся как есть.
  const THEME_PRESETS: { name: string; accent: string; accentSecondary: string }[] = [
    { name: "Indigo", accent: "#6366f1", accentSecondary: "#6366f1" },
    { name: "Океан", accent: "#0891b2", accentSecondary: "#6366f1" },
    { name: "Закат", accent: "#f43f5e", accentSecondary: "#f59e0b" },
    { name: "Лес", accent: "#10b981", accentSecondary: "#65a30d" },
    { name: "Rose", accent: "#f43f5e", accentSecondary: "#f43f5e" },
    { name: "Slate", accent: "#64748b", accentSecondary: "#64748b" },
  ];

  // Применяем тему сразу при любом изменении — живое превью без нажатия «Сохранить».
  function previewTheme() {
    applyTheme(settings.theme_mode, settings);
  }

  function applyPreset(accent: string, accentSecondary: string) {
    settings.color_accent = accent;
    settings.color_accent_secondary = accentSecondary;
    previewTheme();
  }

  function resetColors() {
    settings.color_accent = "";
    settings.color_accent_secondary = "";
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
    color_accent_secondary: "",
    color_bg: "",
    color_text: "",
    color_border: "",
    quiet_until: "",
    context_notifications: true,
    ai_fallback: false,
    openai_in_keyring: false,
    anthropic_in_keyring: false,
    app_category_rules: "",
    app_limits: "",
    auto_backup_dir: "",
    auto_backup_keep: 7,
    morning_digest_time: "",
    show_subtasks_expanded: true,
  });

  let saving = $state(false);
  let saved = $state(false);
  let error: string | null = $state(null);
  let trackingMode: "extended" | "basic" | null = $state(null);
  let windowTracking: string | null = $state(null);

  // --- Поиск по настройкам (v0.8.5): простой substring-match по всему
  // тексту секции, без индексации/fuzzy. Пустой запрос — всё видно.
  let searchQuery = $state("");
  let sectionEls: HTMLElement[] = $state([]);
  let sectionMatches = $state<boolean[]>([]);

  function recomputeSearch() {
    const q = searchQuery.trim().toLowerCase();
    sectionMatches = sectionEls.map(el =>
      !q || (el?.textContent?.toLowerCase().includes(q) ?? true)
    );
  }

  // Правила «класс окна → категория»: редактируются строками,
  // сериализуются в settings.app_category_rules при сохранении.
  let appRules: AppCategoryRule[] = $state([]);
  const RULE_CATEGORIES: { value: AppCategoryRule["category"]; label: string }[] = [
    { value: "Work", label: "Работа" },
    { value: "Study", label: "Учёба" },
    { value: "Home", label: "Дом" },
    { value: "Health", label: "Здоровье" },
    { value: "Other", label: "Другое" },
  ];

  function parseRules(json: string): AppCategoryRule[] {
    try {
      const v = JSON.parse(json);
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  }

  // Лимиты времени на категории приложений: одна запись на категорию,
  // 0/пусто = без лимита. Сериализуются в settings.app_limits при сохранении.
  let appLimits: Record<string, number> = $state({});

  function parseLimits(json: string): AppLimit[] {
    try {
      const v = JSON.parse(json);
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  }

  onMount(async () => {
    try {
      settings = await api.getSettings();
      appRules = parseRules(settings.app_category_rules);
      appLimits = Object.fromEntries(
        parseLimits(settings.app_limits).map(l => [l.category, l.daily_mins])
      );
    } catch (e) {
      error = String(e);
    }
    trackingMode = await api.getTrackingMode().catch(() => null);
    windowTracking = await api.getWindowTracking().catch(() => null);
    categoryStore.load();
  });

  // --- Категории задач (CRUD сохраняется сразу, без кнопки «Сохранить») ---
  let newCatName = $state("");
  let newCatColor = $state("#2a78d6");

  async function addCategory() {
    const name = newCatName.trim();
    if (!name) return;
    await categoryStore.create(name, newCatColor);
    newCatName = "";
  }

  async function save() {
    saving = true;
    error = null;
    try {
      settings.app_category_rules = JSON.stringify(appRules.filter(r => r.pattern.trim()));
      settings.app_limits = JSON.stringify(
        Object.entries(appLimits)
          .filter(([, mins]) => mins > 0)
          .map(([category, daily_mins]) => ({ category, daily_mins }))
      );
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
  let backupNowBusy = $state(false);
  let backupNowMsg = $state("");
  let lastBackup: string | null = $state(null);

  async function pickBackupDir() {
    error = null;
    try {
      const path = await openDialog({ directory: true, multiple: false });
      if (path) settings.auto_backup_dir = path;
    } catch (e) {
      error = String(e);
    }
  }

  async function doBackupNow() {
    backupNowBusy = true;
    backupNowMsg = "";
    try {
      const name = await api.doAutoBackup();
      backupNowMsg = `Бэкап сохранён: ${name}`;
    } catch (e) {
      backupNowMsg = `Ошибка: ${e}`;
    } finally {
      backupNowBusy = false;
    }
  }

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

  // Тест-кнопка: сбросить онбординг и перезагрузить webview — App.svelte
  // перечитает настройки и покажет онбординг сразу. Берём свежие настройки
  // из БД, чтобы не сохранить заодно несохранённые правки формы.
  async function resetOnboarding() {
    error = null;
    try {
      const fresh = await api.getSettings();
      fresh.onboarding_complete = false;
      await api.saveSettings(fresh);
      location.reload();
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

  let notesMdMsg = $state("");

  async function exportNotesMd() {
    notesMdMsg = "";
    error = null;
    try {
      const dir = await openDialog({ directory: true, multiple: false });
      if (!dir) return;
      const count = await api.exportNotesMd(dir as string);
      notesMdMsg = `Экспортировано заметок: ${count}`;
    } catch (e) {
      error = String(e);
    }
  }

  async function importNotesMd() {
    notesMdMsg = "";
    error = null;
    try {
      const dir = await openDialog({ directory: true, multiple: false });
      if (!dir) return;
      const count = await api.importNotesMd(dir as string);
      notesMdMsg = `Импортировано заметок: ${count}. Совпадения по названию создаются как отдельные заметки.`;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="settings">
  <h2 class="page-title" style="margin-bottom:14px;">Настройки</h2>

  <input
    type="search"
    class="settings-search"
    placeholder="Поиск по настройкам…"
    bind:value={searchQuery}
    oninput={recomputeSearch}
  />

  {#if error}
    <div class="alert">{error}</div>
  {/if}

  <section class="card panel" class:hidden-by-search={sectionMatches[0] === false} bind:this={sectionEls[0]}>
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
        <button type="button" class="btn-sm" onclick={() => applyPreset(p.accent, p.accentSecondary)}>
          <span class="swatch" style="background:linear-gradient(135deg, {p.accent}, {p.accentSecondary});"></span>
          {p.name}
        </button>
      {/each}
    </div>

    <div class="color-grid">
      {#each [["color_accent","Акцент"],["color_accent_secondary","Доп. акцент"],["color_bg","Фон"],["color_text","Текст"],["color_border","Границы"]] as [key, label]}
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

    <label class="check" style="margin-top:12px;">
      <input type="checkbox" bind:checked={settings.show_subtasks_expanded} />
      Показывать подзадачи в списке задач развёрнутыми
    </label>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[1] === false} bind:this={sectionEls[1]}>
    <h3 class="section-title">ИИ-провайдер</h3>

    <label class="field">
      <span class="label">Провайдер</span>
      <select bind:value={settings.ai_provider}>
        {#each PROVIDERS as p (p.value)}
          <option value={p.value}>{p.label}</option>
        {/each}
      </select>
    </label>

    {#if settings.ai_provider !== "none"}
      <label class="check" style="margin-top:10px;">
        <input type="checkbox" bind:checked={settings.ai_fallback} />
        Автопереключение: при ошибке или недоступности пробовать других доступных провайдеров
      </label>
    {/if}

    <!-- Один блок настроек, поля зависят от выбранного провайдера — не два
         параллельных дублирующих блока, как было при radio-списке. -->
    {#if settings.ai_provider === "openai" || settings.ai_provider === "anthropic"}
      {@const isOpenai = settings.ai_provider === "openai"}
      <div class="stack" style="margin-top:12px;">
        <label class="field">
          <span class="label">API Key
            {#if isOpenai ? settings.openai_key : settings.anthropic_key}
              {#if isOpenai ? settings.openai_in_keyring : settings.anthropic_in_keyring}
                <span class="key-ok"><Icon name="lock" size={11} /> keyring</span>
              {:else}
                <span class="key-warn">⚠ БД (keyring недоступен)</span>
              {/if}
            {/if}
          </span>
          {#if isOpenai}
            <input type="password" bind:value={settings.openai_key} placeholder="sk-..." />
          {:else}
            <input type="password" bind:value={settings.anthropic_key} placeholder="sk-ant-..." />
          {/if}
        </label>
        <label class="field">
          <span class="label">Модель</span>
          {#if isOpenai}
            <select bind:value={settings.openai_model}>
              <option value="gpt-4o-mini">gpt-4o-mini (быстрый, дешёвый)</option>
              <option value="gpt-4o">gpt-4o</option>
              <option value="gpt-4-turbo">gpt-4-turbo</option>
            </select>
          {:else}
            <select bind:value={settings.anthropic_model}>
              <option value="claude-haiku-4-5-20251001">claude-haiku-4-5 (быстрый, дешёвый)</option>
              <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
            </select>
          {/if}
        </label>
      </div>
    {:else if settings.ai_provider === "local"}
      <div style="margin-top:12px;">
        <p class="muted" style="font-size:12px;margin:0 0 10px 0;">
          Локальная модель хранится в <code>~/.local/share/ai-notes/models/model.gguf</code>
        </p>
        <ModelDownloader />
      </div>
    {/if}
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[2] === false} bind:this={sectionEls[2]}>
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

  <section class="card panel" class:hidden-by-search={sectionMatches[3] === false} bind:this={sectionEls[3]}>
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
    {#if trackingMode}
      <p class="hint">
        Режим трекинга: {trackingMode === "extended"
          ? "расширенный — системный простой/возврат от композитора (ext-idle-notify)"
          : "базовый — только ввод в окне приложения"}
        {windowTracking ? ` · приложения: ${windowTracking}` : ""}
      </p>
    {/if}

    {#if windowTracking}
      <div class="sub-label" style="margin-top:12px;">Категории приложений (класс окна → категория)</div>
      {#each appRules as rule, i}
        <div class="rule-row">
          <input bind:value={rule.pattern} placeholder="класс окна, напр. jetbrains-*" />
          <select bind:value={rule.category}>
            {#each RULE_CATEGORIES as c}
              <option value={c.value}>{c.label}</option>
            {/each}
          </select>
          <button class="btn-icon btn-danger" title="Удалить правило"
            onclick={() => appRules = appRules.filter((_, j) => j !== i)}>✕</button>
        </div>
      {/each}
      <button class="btn-sm" onclick={() => appRules = [...appRules, { pattern: "", category: "Work" }]}>
        + Правило
      </button>
      <p class="hint">
        Первое совпавшее правило выигрывает; <code>*</code> — любая подстрока.
        Приложения без правила попадают в «Другое». Применяется после «Сохранить».
      </p>

      <div class="sub-label" style="margin-top:12px;">Лимиты времени на категории (мин/день)</div>
      {#each RULE_CATEGORIES as c}
        <div class="rule-row limit-row">
          <span class="muted" style="flex:1;">{c.label}</span>
          <input
            type="number" min="0" style="width:90px;"
            placeholder="без лимита"
            value={appLimits[c.value] || ""}
            oninput={(e) => {
              const n = parseInt((e.currentTarget as HTMLInputElement).value, 10);
              appLimits = { ...appLimits, [c.value]: Number.isFinite(n) ? n : 0 };
            }}
          />
        </div>
      {/each}
      <p class="hint">
        0 или пусто — без лимита. При превышении — уведомление раз в день
        (пока лимит остаётся превышенным). Применяется после «Сохранить».
      </p>
    {/if}
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[4] === false} bind:this={sectionEls[4]}>
    <h3 class="section-title">Категории задач</h3>
    {#each categoryStore.categories as c (c.id)}
      <div class="rule-row">
        <input
          type="color"
          class="cat-color"
          value={c.color}
          title="Цвет категории"
          onchange={(e) => categoryStore.update(c.id, { color: e.currentTarget.value })}
        />
        <input
          value={c.name}
          onchange={(e) => {
            const name = e.currentTarget.value.trim();
            if (name && name !== c.name) categoryStore.update(c.id, { name });
            else e.currentTarget.value = c.name;
          }}
        />
        {#if c.id !== "Other"}
          <button class="btn-icon btn-danger" title="Удалить (задачи перейдут в «Другое»)"
            onclick={() => categoryStore.remove(c.id)}>✕</button>
        {:else}
          <span class="hint" style="margin:0;">фолбэк</span>
        {/if}
      </div>
    {/each}
    <div class="rule-row">
      <input type="color" class="cat-color" bind:value={newCatColor} title="Цвет новой категории" />
      <input bind:value={newCatName} placeholder="Новая категория"
        onkeydown={(e) => { if (e.key === "Enter") addCategory(); }} />
      <button class="btn-sm" onclick={addCategory} disabled={!newCatName.trim()}>Добавить</button>
    </div>
    {#if categoryStore.error}
      <p class="hint" style="color:var(--danger, #d33);">{categoryStore.error}</p>
    {/if}
    <p class="hint">
      Изменения сохраняются сразу. При удалении категории её задачи переходят в «Другое».
    </p>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[5] === false} bind:this={sectionEls[5]}>
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
    <label class="field" style="margin-top:8px;">
      <span class="label">Утренняя сводка (HH:MM, пусто = выкл)</span>
      <input type="time" bind:value={settings.morning_digest_time} />
    </label>
    <p class="hint">
      Пауза всех уведомлений — в меню трея: «Пауза уведомлений» (30 мин / 1 ч / 2 ч / бессрочно).
    </p>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[6] === false} bind:this={sectionEls[6]}>
    <h3 class="section-title">Авто-бэкап</h3>
    <div class="stack">
      <label class="field">
        <span class="label">Папка для бэкапов (пусто = выкл)</span>
        <div class="input-row">
          <input type="text" bind:value={settings.auto_backup_dir} placeholder="Выберите папку..." readonly style="flex:1;" />
          <button class="btn-sm" onclick={pickBackupDir}>Обзор…</button>
        </div>
      </label>
      <label class="field">
        <span class="label">Хранить копий</span>
        <input type="number" min="1" bind:value={settings.auto_backup_keep} />
      </label>
      {#if lastBackup}
        <p class="hint">Последний бэкап: {lastBackup}</p>
      {/if}
      <div class="preset-row">
        <button class="btn-sm" onclick={doBackupNow} disabled={backupNowBusy || !settings.auto_backup_dir.trim()}>
          {backupNowBusy ? "…" : "Сделать сейчас"}
        </button>
        {#if backupNowMsg}
          <span class="muted" style="font-size:12px;">{backupNowMsg}</span>
        {/if}
      </div>
    </div>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[7] === false} bind:this={sectionEls[7]}>
    <h3 class="section-title">Данные</h3>
    <div class="preset-row">
      <button class="btn-sm" onclick={exportData}>Экспорт (ZIP)</button>
      <button class="btn-sm" onclick={importData}>Импорт (ZIP)</button>
      <button class="btn-sm" onclick={resetOnboarding} title="Сбросит флаг onboarding_complete и покажет онбординг заново">Сбросить онбординг</button>
      {#if backupMsg}
        <span class="muted" style="font-size:12px;">{backupMsg}</span>
      {/if}
    </div>
    <div class="preset-row" style="margin-top:8px;">
      <button class="btn-sm" onclick={exportNotesMd}>Экспорт заметок (.md)</button>
      <button class="btn-sm" onclick={importNotesMd}>Импорт заметок из папки</button>
      {#if notesMdMsg}
        <span class="muted" style="font-size:12px;">{notesMdMsg}</span>
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

  .settings-search {
    width: 100%;
    margin-bottom: 14px;
  }

  .hidden-by-search {
    display: none;
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

  .input-row {
    display: flex;
    gap: 6px;
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

  .rule-row {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-bottom: 6px;
  }

  .rule-row input {
    flex: 1;
    min-width: 0;
  }

  .rule-row input.cat-color {
    flex: 0 0 34px;
    width: 34px;
    height: 26px;
    padding: 1px 2px;
    cursor: pointer;
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
