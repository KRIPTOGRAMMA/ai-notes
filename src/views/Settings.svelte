<script lang="ts">
  import { onMount } from "svelte";
  import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api/tauri";
  import { categoryStore } from "../lib/stores/categories.svelte";
  import { statusStore } from "../lib/stores/statuses.svelte";
  import type { AppSettings, AppCategoryRule, AppLimit, GlobalAction } from "../lib/types";
  import { applyTheme } from "../lib/theme";
  import ModelDownloader from "../lib/components/ModelDownloader.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import { HELP_TOPICS } from "../lib/help";
  import { LANGS, SEEDED_CATEGORY_IDS, type Lang } from "../lib/i18n";
  import { i18n, t } from "../lib/i18n.svelte";
  import {
    KEYBIND_ACTIONS, type Keybinds,
    parseKeybinds, comboFor, comboFromEvent, formatCombo, findConflicts,
  } from "../lib/keybinds";

  const PROVIDERS: { value: AppSettings["ai_provider"]; label: string }[] = $derived([
    { value: "none", label: t("Без ИИ (функции отключены)") },
    { value: "local", label: t("Локальная модель (llamafile)") },
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic" },
  ]);

  // Каждый пресет задаёт пару акцентов (основной + дополнительный, градиент
  // на .btn-primary) одной кнопкой; «Свой» — ручной выбор ниже остаётся как есть.
  const THEME_PRESETS: { name: string; accent: string; accentSecondary: string }[] = $derived([
    { name: "Indigo", accent: "#6366f1", accentSecondary: "#6366f1" },
    { name: t("Океан"), accent: "#0891b2", accentSecondary: "#6366f1" },
    { name: t("Закат"), accent: "#f43f5e", accentSecondary: "#f59e0b" },
    { name: t("Лес"), accent: "#10b981", accentSecondary: "#65a30d" },
    { name: "Rose", accent: "#f43f5e", accentSecondary: "#f43f5e" },
    { name: "Slate", accent: "#64748b", accentSecondary: "#64748b" },
  ]);

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
    keybinds: "",
    global_keybinds: "",
    focus_mode_auto: true,
    track_domains: false,
    language: "",
    history_cleanup_months: 0,
  });

  let saving = $state(false);
  let saved = $state(false);
  let error: string | null = $state(null);
  let trackingMode: "extended" | "basic" | null = $state(null);
  let windowTracking: string | null = $state(null);
  let modelPath: string | null = $state(null);
  // Число очищенных записей о доменах — показывается после нажатия, чтобы
  // действие не выглядело как «ничего не произошло» (v0.9.31).
  let domainCleared: number | null = $state(null);

  async function clearDomains() {
    domainCleared = await api.clearDomainHistory().catch(() => null);
  }

  // --- Вкладки (v0.8.10): секции сгруппированы, чтобы не листать одну
  // длинную колонку. SECTION_TAB[i] — id вкладки для секции с индексом i
  // (индексы секций те же, что использует sectionEls/sectionMatches ниже).
  // Подписи через $derived, а не обычным const: язык меняется без перезагрузки
  // (v0.9.32), а const вычислился бы один раз при загрузке модуля и оставил
  // вкладки на прежнем языке (v0.9.46).
  const TAB_IDS = ["general", "ai", "tasks", "notifications", "data", "hotkeys", "help"] as const;
  type TabId = (typeof TAB_IDS)[number];
  const TABS = $derived<{ id: TabId; label: string }[]>([
    { id: "general", label: t("Общее") },
    { id: "ai", label: t("ИИ") },
    { id: "tasks", label: t("Категории") },
    { id: "notifications", label: t("Уведомления") },
    { id: "data", label: t("Данные") },
    { id: "hotkeys", label: t("Хоткеи") },
    { id: "help", label: t("Справка") },
  ]);
  // Внешний вид(0), Режим работы(2) → Общее; ИИ-провайдер(1) → ИИ;
  // Мониторинг(3), Категории задач(4) → Задачи; Уведомления(5) → Уведомления;
  // Авто-бэкап(6), Данные(7) → Данные; Хоткеи(8) → Хоткеи; Статусы(9) →
  // Задачи (добавлена последней по индексу, чтобы не перенумеровывать
  // существующие секции, но логически сгруппирована с Категориями).
  // Справка(10) → Справка (v0.9.29 — добавлена последней по индексу,
  // чтобы не перенумеровывать существующие секции).
  const SECTION_TAB: TabId[] = ["general", "ai", "general", "tasks", "tasks", "notifications", "data", "data", "hotkeys", "tasks", "help"];
  let activeTab = $state<TabId>("general");

  // --- Поиск по настройкам (v0.8.5): простой substring-match по всему
  // тексту секции, без индексации/fuzzy. Пустой запрос — всё видно везде;
  // непустой — автоматически переключает на первую вкладку с совпадением
  // (v0.8.10), внутри вкладки несовпавшие секции по-прежнему скрыты.
  let searchQuery = $state("");
  let sectionEls: HTMLElement[] = $state([]);
  let sectionMatches = $state<boolean[]>([]);
  // При активном поиске темы справки раскрыты: иначе совпадение лежит в
  // свёрнутом <details> и пользователь видит тему без видимого текста.
  let helpSearchOpen = $derived(searchQuery.trim() !== "");

  function recomputeSearch() {
    const q = searchQuery.trim().toLowerCase();
    sectionMatches = sectionEls.map(el =>
      !q || (el?.textContent?.toLowerCase().includes(q) ?? true)
    );
    if (q) {
      const firstMatch = sectionMatches.findIndex(m => m);
      if (firstMatch >= 0) activeTab = SECTION_TAB[firstMatch];
    }
  }

  // Правила «класс окна → категория»: редактируются строками,
  // сериализуются в settings.app_category_rules при сохранении.
  let appRules: AppCategoryRule[] = $state([]);
  const RULE_CATEGORIES: { value: AppCategoryRule["category"]; label: string }[] = $derived([
    { value: "Work", label: t("Работа") },
    { value: "Study", label: t("Учёба") },
    { value: "Home", label: t("Дом") },
    { value: "Health", label: t("Здоровье") },
    { value: "Other", label: t("Другое") },
  ]);

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
      // Пустая настройка = язык не выбирался явно. В селекте показываем
      // фактически действующий язык (его определил i18n.init по локали),
      // иначе поле выглядело бы пустым при работающем переводе.
      if (!settings.language) settings.language = i18n.lang;
      appRules = parseRules(settings.app_category_rules);
      appLimits = Object.fromEntries(
        parseLimits(settings.app_limits).map(l => [l.category, l.daily_mins])
      );
      keybinds = parseKeybinds(settings.keybinds);
      globalBinds = parseKeybinds(settings.global_keybinds);
    } catch (e) {
      error = String(e);
    }
    // Список глобальных действий — с бэкенда: он их и регистрирует.
    globalActions = await api.listGlobalActions().catch(() => []);
    trackingMode = await api.getTrackingMode().catch(() => null);
    windowTracking = await api.getWindowTracking().catch(() => null);
    // Реальный путь от бэкенда, а не собранная на фронте строка: каталог
    // зависит от ОС (app_data_dir) и от identifier'а приложения (v0.9.28).
    modelPath = await api.modelPath().catch(() => null);
    categoryStore.load();
    statusStore.load();
  });

  // --- Хоткеи (v0.8.9): оверрайды хранятся в settings.keybinds (JSON),
  // дефолты — KEYBIND_ACTIONS.defaultCombo. Запись нового бинда — по клику
  // на «Записать», следующее нажатие клавиш (не модификатор) фиксируется.
  let keybinds: Keybinds = $state({});
  let recordingActionId: string | null = $state(null);
  let keybindConflict: { actionId: string; withLabel: string } | null = $state(null);

  // v0.9.35: пока идёт запись, App.svelte не выполняет хоткеи — иначе запись
  // комбинации, уже занятой локальным действием (Ctrl+K), выполняла бы это
  // действие и уводила фокус из поля записи.
  function setRecordingFlag(on: boolean) {
    window.dispatchEvent(new CustomEvent("keybind-recording", { detail: on }));
  }

  function startRecording(actionId: string) {
    recordingActionId = actionId;
    keybindConflict = null;
    setRecordingFlag(true);
  }

  function onKeybindCapture(e: KeyboardEvent) {
    if (!recordingActionId) return;
    e.preventDefault();
    if (e.key === "Escape") { recordingActionId = null; setRecordingFlag(false); return; }
    const combo = comboFromEvent(e);
    if (!combo) return; // нажат только модификатор — ждём основную клавишу

    const conflicts = findConflicts(keybinds, recordingActionId, combo);
    if (conflicts.length > 0) {
      const other = KEYBIND_ACTIONS.find(a => a.id === conflicts[0]);
      keybindConflict = { actionId: recordingActionId, withLabel: other?.label ?? conflicts[0] };
      return;
    }
    keybinds = { ...keybinds, [recordingActionId]: combo };
    recordingActionId = null;
    keybindConflict = null;
    setRecordingFlag(false);
  }

  function resetKeybind(actionId: string) {
    const { [actionId]: _drop, ...rest } = keybinds;
    keybinds = rest;
  }

  // --- Глобальные хоткеи (v0.9.35) ---
  //
  // Формат комбинации тот же, что у webview-хоткеев ("Ctrl+Shift+KeyN"):
  // специально проверено, что парсер global-hotkey понимает и такой вид, и
  // "Ctrl+Shift+N" — конвертер между форматами не нужен, а запись комбинации
  // в UI одна и та же для обеих групп.
  //
  // Отличий от локальных три: список действий приходит с бэкенда (он же их
  // регистрирует), комбинацию проверяет бэкенд, а после сохранения нужна
  // перерегистрация — иначе новая комбинация заработает только после
  // перезапуска приложения.
  let globalActions: GlobalAction[] = $state([]);
  let globalBinds: Keybinds = $state({});
  let recordingGlobalId: string | null = $state(null);
  let globalError: { actionId: string; text: string } | null = $state(null);
  // Комбинации, которые ОС отказалась отдать (заняты другим приложением или
  // композитором). Показываются отдельно: это не ошибка ввода, а факт среды.
  let globalFailed: string[] = $state([]);

  function globalComboFor(actionId: string): string {
    const a = globalActions.find(x => x.id === actionId);
    return globalBinds[actionId] ?? a?.default_combo ?? "";
  }

  function startRecordingGlobal(actionId: string) {
    recordingGlobalId = actionId;
    globalError = null;
    setRecordingFlag(true);
  }

  async function onGlobalCapture(e: KeyboardEvent) {
    if (!recordingGlobalId) return;
    e.preventDefault();
    if (e.key === "Escape") { recordingGlobalId = null; setRecordingFlag(false); return; }
    const combo = comboFromEvent(e);
    if (!combo) return; // только модификатор — ждём основную клавишу

    const actionId = recordingGlobalId;

    // Конфликт внутри группы: две глобальные команды на одной комбинации ОС
    // не различит.
    const dupe = globalActions.find(a => a.id !== actionId && globalComboFor(a.id) === combo);
    if (dupe) {
      globalError = { actionId, text: t("Уже занято: {label}", { label: dupe.label }) };
      return;
    }
    // Конфликт с локальным хоткеем: глобальный перехватывает клавиши раньше,
    // поэтому локальный просто перестал бы работать — молча и необъяснимо.
    const localDupe = KEYBIND_ACTIONS.find(a => comboFor(keybinds, a.id) === combo);
    if (localDupe) {
      globalError = { actionId, text: t("Занято хоткеем в приложении: {label}", { label: localDupe.label }) };
      return;
    }
    // Последнее слово — за настоящим парсером комбинаций, а не за нашими
    // правилами: регистрировать будет именно он.
    try {
      await api.validateGlobalCombo(combo);
    } catch (err) {
      // Пока ждали ответ, пользователь мог выйти из записи (Escape) или
      // начать записывать другое действие — тогда ответ уже неактуален и
      // показывать по нему ошибку нельзя: она вернула бы поле в режим записи.
      if (recordingGlobalId !== actionId) return;
      globalError = { actionId, text: typeof err === "string" ? err : t("Комбинация не подходит") };
      return;
    }
    if (recordingGlobalId !== actionId) return;

    globalBinds = { ...globalBinds, [actionId]: combo };
    recordingGlobalId = null;
    globalError = null;
    setRecordingFlag(false);
  }

  function resetGlobalKeybind(actionId: string) {
    const { [actionId]: _drop, ...rest } = globalBinds;
    globalBinds = rest;
  }

  // --- Категории задач (CRUD сохраняется сразу, без кнопки «Сохранить») ---
  let newCatName = $state("");
  let newCatColor = $state("#2a78d6");

  async function addCategory() {
    const name = newCatName.trim();
    if (!name) return;
    await categoryStore.create(name, newCatColor);
    newCatName = "";
  }

  // --- Статусы задач (v0.9.20, канбан) — тот же паттерн, что категории:
  // CRUD сохраняется сразу, без кнопки «Сохранить». Todo/InProgress/Done/
  // Archived зарезервированы (is_reserved) — не переименовываются/не удаляются.
  let newStatusName = $state("");
  let newStatusColor = $state("#2a78d6");

  async function addStatus() {
    const name = newStatusName.trim();
    if (!name) return;
    await statusStore.create(name, newStatusColor);
    newStatusName = "";
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
      settings.keybinds = JSON.stringify(keybinds);
      settings.global_keybinds = JSON.stringify(globalBinds);
      await api.saveSettings(settings);
      // Перерегистрация в ОС: без неё новая комбинация заработала бы только
      // после перезапуска, а старая продолжала бы срабатывать.
      globalFailed = await api.applyGlobalHotkeys().catch(() => []);
      applyTheme(settings.theme_mode, settings);
      // App.svelte держит свою копию хоткеев для keydown-обработчика —
      // без этого события переназначение применялось бы только после reload.
      window.dispatchEvent(new CustomEvent("keybinds-saved", { detail: settings.keybinds }));
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
      backupNowMsg = t("Бэкап сохранён: {name}", { name });
    } catch (e) {
      backupNowMsg = t("Ошибка: {e}", { e: String(e) });
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
      backupMsg = t("Экспорт завершён ✓");
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
    if (!confirm(t("Импорт заменит все текущие данные. Продолжить?"))) return;
    try {
      const path = await openDialog({
        multiple: false,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (!path) return;
      await api.importData(path as string);
      backupMsg = t("Импорт завершён ✓ Приложение перезапускается...");
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
      notesMdMsg = t("Экспортировано заметок: {n}", { n: count });
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
      notesMdMsg = t("Импортировано заметок: {n}. Совпадения по названию создаются как отдельные заметки.", { n: count });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="settings">
  <h2 class="page-title" style="margin-bottom:14px;">{t("Настройки")}</h2>

  <input
    type="search"
    class="settings-search"
    placeholder={t("Поиск по настройкам…")}
    bind:value={searchQuery}
    oninput={recomputeSearch}
  />

  <div class="settings-tabs" role="tablist">
    {#each TABS as tab (tab.id)}
      <button
        type="button"
        class="settings-tab"
        class:active={activeTab === tab.id}
        role="tab"
        aria-selected={activeTab === tab.id}
        onclick={() => activeTab = tab.id}
      >{tab.label}</button>
    {/each}
  </div>

  {#if error}
    <div class="alert">{error}</div>
  {/if}

  <section class="card panel" class:hidden-by-search={sectionMatches[0] === false} class:hidden-by-tab={SECTION_TAB[0] !== activeTab} bind:this={sectionEls[0]}>
    <h3 class="section-title">{t("Внешний вид")}</h3>

    <!-- Язык (v0.9.32): применяется сразу, без «Сохранить» — как и тема.
         Для языка это важнее, чем для темы: увидеть результат до сохранения
         единственный способ понять, что выбрал правильно. -->
    <label class="field">
      {t("Язык")}
      <select bind:value={settings.language} onchange={() => i18n.set(settings.language as Lang)}>
        {#each LANGS as l (l.id)}
          <option value={l.id}>{l.label}</option>
        {/each}
      </select>
    </label>

    <div class="radio-row">
      {#each [["light", t("Светлая")], ["dark", t("Тёмная")], ["system", t("Системная")]] as [val, label]}
        <label class="check">
          <input type="radio" name="theme_mode" value={val} bind:group={settings.theme_mode} onchange={previewTheme} />
          {label}
        </label>
      {/each}
    </div>

    <div class="sub-label">{t("Пресеты акцента")}</div>
    <div class="preset-row">
      {#each THEME_PRESETS as p}
        <button type="button" class="btn-sm" onclick={() => applyPreset(p.accent, p.accentSecondary)}>
          <span class="swatch" style="background:linear-gradient(135deg, {p.accent}, {p.accentSecondary});"></span>
          {p.name}
        </button>
      {/each}
    </div>

    <div class="color-grid">
      {#each [["color_accent",t("Акцент")],["color_accent_secondary",t("Доп. акцент")],["color_bg",t("Фон")],["color_text",t("Текст")],["color_border",t("Границы")]] as [key, label]}
        <label class="check">
          <input type="color"
            value={(settings as any)[key] || "#6366f1"}
            oninput={(e) => { (settings as any)[key] = e.currentTarget.value; previewTheme(); }}
            class="color-input" />
          {label}
        </label>
      {/each}
    </div>

    <button type="button" class="btn-sm" style="margin-top:10px;" onclick={resetColors}>{t("Сбросить к дефолту")}</button>

    <label class="check" style="margin-top:12px;">
      <input type="checkbox" bind:checked={settings.show_subtasks_expanded} />{t("Показывать подзадачи в списке задач развёрнутыми")}</label>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[1] === false} class:hidden-by-tab={SECTION_TAB[1] !== activeTab} bind:this={sectionEls[1]}>
    <h3 class="section-title">{t("ИИ-провайдер")}</h3>

    <label class="field">
      <span class="label">{t("Провайдер")}</span>
      <select bind:value={settings.ai_provider}>
        {#each PROVIDERS as p (p.value)}
          <option value={p.value}>{p.label}</option>
        {/each}
      </select>
    </label>

    {#if settings.ai_provider !== "none"}
      <label class="check" style="margin-top:10px;">
        <input type="checkbox" bind:checked={settings.ai_fallback} />{t("Автопереключение: при ошибке или недоступности пробовать других доступных провайдеров")}</label>
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
                <span class="key-warn">{t("⚠ БД (keyring недоступен)")}</span>
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
          <span class="label">{t("Модель")}</span>
          {#if isOpenai}
            <select bind:value={settings.openai_model}>
              <option value="gpt-4o-mini">{t("gpt-4o-mini (быстрый, дешёвый)")}</option>
              <option value="gpt-4o">gpt-4o</option>
              <option value="gpt-4-turbo">gpt-4-turbo</option>
            </select>
          {:else}
            <select bind:value={settings.anthropic_model}>
              <option value="claude-haiku-4-5-20251001">{t("claude-haiku-4-5 (быстрый, дешёвый)")}</option>
              <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
            </select>
          {/if}
        </label>
      </div>
    {:else if settings.ai_provider === "local"}
      <div style="margin-top:12px;">
        <p class="muted" style="font-size:12px;margin:0 0 10px 0;">{t("Локальная модель хранится в")}<code>{modelPath ?? "…"}</code>
        </p>
        <ModelDownloader />
      </div>
    {/if}
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[2] === false} class:hidden-by-tab={SECTION_TAB[2] !== activeTab} bind:this={sectionEls[2]}>
    <h3 class="section-title">{t("Режим работы")}</h3>
    <select bind:value={settings.work_mode} style="width:100%;">
      <option value="Light">{t("Light — обычный режим")}</option>
      <option value="Focus">{t("Focus — без уведомлений")}</option>
      <option value="Study">{t("Study — помодоро-сессии (25/5)")}</option>
    </select>
    <p class="hint">{t("Применяется сразу после сохранения.")}</p>

    {#if settings.work_mode === "Study"}
      <div class="pair" style="margin-top:10px;">
        <label class="field">
          <span class="label">{t("Рабочий блок (мин)")}</span>
          <input type="number" min="1" max="120" bind:value={settings.pomodoro_work_mins} />
        </label>
        <label class="field">
          <span class="label">{t("Перерыв (мин)")}</span>
          <input type="number" min="1" max="60" bind:value={settings.pomodoro_break_mins} />
        </label>
      </div>
      <p class="hint">{t("Применяется при следующем входе в режим Study.")}</p>
    {/if}
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[3] === false} class:hidden-by-tab={SECTION_TAB[3] !== activeTab} bind:this={sectionEls[3]}>
    <h3 class="section-title">{t("Мониторинг")}</h3>
    <div class="pair">
      <label class="field">
        <span class="label">{t("Порог простоя (сек, мин. 60)")}</span>
        <input type="number" min="60" bind:value={settings.idle_threshold_secs} />
      </label>
      <label class="field">
        <span class="label">{t("Интервал логирования (сек, 10–600)")}</span>
        <input type="number" min="10" max="600" bind:value={settings.log_interval_secs} />
      </label>
    </div>
    <p class="hint">{t("Применяется после перезапуска приложения.")}</p>
    {#if trackingMode}
      <p class="hint">
        {t("Режим трекинга")}: {trackingMode === "extended"
          ? t("расширенный — системный простой/возврат от композитора (ext-idle-notify)")
          : t("базовый — только ввод в окне приложения")}
        {windowTracking ? ` · ${t("приложения")}: ${windowTracking}` : ""}
      </p>
    {/if}

    <!-- Домены (v0.9.31): показывается там же, где работает трекинг окон —
         без провайдера заголовок читать неоткуда, и галочка была бы мёртвой.
         Формулировка намеренно прямая: пользователь должен понимать, что
         именно начнёт происходить, а не увидеть безобидное «улучшить
         статистику». -->
    {#if windowTracking}
      <label class="option" style="margin-top:12px;align-items:flex-start;">
        <input type="checkbox" bind:checked={settings.track_domains} />
        <span>{t("Разбивать браузерное время по сайтам")}
          <br /><small class="hint" style="margin:0;">
            {t("Требует чтения заголовков окон браузера. В базу сохраняется")}
            <b>{t("только домен")}</b> {t("(github.com), сам заголовок — название вкладки, поисковый запрос — не сохраняется никогда. Выключено по умолчанию.")}
          </small>
        </span>
      </label>
      {#if domainCleared !== null}
        <p class="hint">{t("Очищено записей: {n}", { n: domainCleared })}</p>
      {/if}
      <button class="btn-sm" style="margin-top:6px;" onclick={clearDomains}>{t("Забыть собранные домены")}</button>

      <div class="sub-label" style="margin-top:12px;">{t("Категории приложений (класс окна → категория)")}</div>
      {#each appRules as rule, i}
        <div class="rule-row">
          <input bind:value={rule.pattern} placeholder={t("класс окна, напр. jetbrains-*")} />
          <select bind:value={rule.category}>
            {#each RULE_CATEGORIES as c}
              <option value={c.value}>{c.label}</option>
            {/each}
          </select>
          <button class="btn-icon btn-danger" title={t("Удалить правило")}
            onclick={() => appRules = appRules.filter((_, j) => j !== i)}>✕</button>
        </div>
      {/each}
      <button class="btn-sm" onclick={() => appRules = [...appRules, { pattern: "", category: "Work" }]}>{t("+ Правило")}</button>
 <p class="hint">{t("Первое совпавшее правило выигрывает;")}<code>*</code>{t("— любая подстрока. Приложения без правила попадают в «Другое». Применяется после «Сохранить».")}</p>

      <div class="sub-label" style="margin-top:12px;">{t("Лимиты времени на категории (мин/день)")}</div>
      {#each RULE_CATEGORIES as c}
        <div class="rule-row limit-row">
          <span class="muted" style="flex:1;">{c.label}</span>
          <input
            type="number" min="0" style="width:90px;"
            placeholder={t("без лимита")}
            value={appLimits[c.value] || ""}
            oninput={(e) => {
              const n = parseInt((e.currentTarget as HTMLInputElement).value, 10);
              appLimits = { ...appLimits, [c.value]: Number.isFinite(n) ? n : 0 };
            }}
          />
        </div>
      {/each}
 <p class="hint">{t("0 или пусто — без лимита. При превышении — уведомление раз в день (пока лимит остаётся превышенным). Применяется после «Сохранить».")}</p>
    {/if}
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[4] === false} class:hidden-by-tab={SECTION_TAB[4] !== activeTab} bind:this={sectionEls[4]}>
    <h3 class="section-title">{t("Категории задач")}</h3>
    {#each categoryStore.categories as c (c.id)}
      <div class="rule-row">
        <input
          type="color"
          class="cat-color"
          value={c.color}
          title={t("Цвет категории")}
          onchange={(e) => categoryStore.update(c.id, { color: e.currentTarget.value })}
        />
        <!--
          Показываем переведённое имя, но посевные категории тогда нельзя
          редактировать (v0.9.47): поле привязано к тому же значению, что
          уходит в БД, и перевод перезаписал бы русский оригинал навсегда.
          Тот же приём, что у статусов ниже с is_reserved. Флага у категорий
          нет, признак посевной — латинский id (uuid у пользовательских).
        -->
        <input
          value={categoryStore.name(c.id)}
          disabled={SEEDED_CATEGORY_IDS.has(c.id)}
          title={SEEDED_CATEGORY_IDS.has(c.id) ? t("Встроенная категория — название нельзя менять") : ""}
          onchange={(e) => {
            const name = e.currentTarget.value.trim();
            if (name && name !== c.name) categoryStore.update(c.id, { name });
            else e.currentTarget.value = c.name;
          }}
        />
        {#if c.id !== "Other"}
          <button class="btn-icon btn-danger" title={t("Удалить (задачи перейдут в «Другое»)")}
            onclick={() => categoryStore.remove(c.id)}>✕</button>
        {:else}
          <span class="hint" style="margin:0;">{t("фолбэк")}</span>
        {/if}
      </div>
    {/each}
    <div class="rule-row">
      <input type="color" class="cat-color" bind:value={newCatColor} title={t("Цвет новой категории")} />
      <input bind:value={newCatName} placeholder={t("Новая категория")}
        onkeydown={(e) => { if (e.key === "Enter") addCategory(); }} />
      <button class="btn-sm" onclick={addCategory} disabled={!newCatName.trim()}>{t("Добавить")}</button>
    </div>
    {#if categoryStore.error}
      <p class="hint" style="color:var(--danger, #d33);">{categoryStore.error}</p>
    {/if}
    <p class="hint">{t("Изменения сохраняются сразу. При удалении категории её задачи переходят в «Другое».")}</p>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[5] === false} class:hidden-by-tab={SECTION_TAB[5] !== activeTab} bind:this={sectionEls[5]}>
    <h3 class="section-title">{t("Уведомления")}</h3>
    <div class="pair">
      <label class="field">
        <span class="label">{t("Первое предупреждение (часов до дедлайна)")}</span>
        <input type="number" min="1" bind:value={settings.deadline_warn_hours} />
      </label>
      <label class="field">
        <span class="label">{t("Второе предупреждение (минут до дедлайна)")}</span>
        <input type="number" min="1" max="1440" bind:value={settings.deadline_warn_minutes} />
      </label>
      <label class="field">
        <span class="label">{t("Возврат после простоя (мин, мин. 1)")}</span>
        <input type="number" min="1" bind:value={settings.idle_notify_min_mins} />
      </label>
      <label class="field">
        <span class="label">{t("Перерыв после N минут работы (0 — выкл)")}</span>
        <input type="number" min="0" bind:value={settings.nudge_after_mins} />
      </label>
    </div>
    <label class="check" style="margin-top:10px;">
      <input type="checkbox" bind:checked={settings.context_notifications} />{t("Контекстные уведомления (накопились просрочки, возврат к задаче «в работе»)")}</label>
    <label class="check" style="margin-top:6px;">
      <input type="checkbox" bind:checked={settings.focus_mode_auto} />{t("Фокус-режим: авто-пауза уведомлений на время помодоро-работы и активных тайм-блоков")}</label>
    <label class="field" style="margin-top:8px;">
      <span class="label">{t("Утренняя сводка (HH:MM, пусто = выкл)")}</span>
      <input type="time" bind:value={settings.morning_digest_time} />
    </label>
    <p class="hint">{t("Пауза всех уведомлений — в меню трея: «Пауза уведомлений» (30 мин / 1 ч / 2 ч / бессрочно).")}</p>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[6] === false} class:hidden-by-tab={SECTION_TAB[6] !== activeTab} bind:this={sectionEls[6]}>
    <h3 class="section-title">{t("Авто-бэкап")}</h3>
    <div class="stack">
      <label class="field">
        <span class="label">{t("Папка для бэкапов (пусто = выкл)")}</span>
        <div class="input-row">
          <input type="text" bind:value={settings.auto_backup_dir} placeholder={t("Выберите папку...")} readonly style="flex:1;" />
          <button class="btn-sm" onclick={pickBackupDir}>{t("Обзор…")}</button>
        </div>
      </label>
      <label class="field">
        <span class="label">{t("Хранить копий")}</span>
        <input type="number" min="1" bind:value={settings.auto_backup_keep} />
      </label>
      {#if lastBackup}
        <p class="hint">{t("Последний бэкап: {d}", { d: lastBackup })}</p>
      {/if}
      <div class="preset-row">
        <button class="btn-sm" onclick={doBackupNow} disabled={backupNowBusy || !settings.auto_backup_dir.trim()}>
          {backupNowBusy ? "…" : t("Сделать сейчас")}
        </button>
        {#if backupNowMsg}
          <span class="muted" style="font-size:12px;">{backupNowMsg}</span>
        {/if}
      </div>
    </div>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[7] === false} class:hidden-by-tab={SECTION_TAB[7] !== activeTab} bind:this={sectionEls[7]}>
    <h3 class="section-title">{t("Данные")}</h3>
    <div class="preset-row">
      <button class="btn-sm" onclick={exportData}>{t("Экспорт (ZIP)")}</button>
      <button class="btn-sm" onclick={importData}>{t("Импорт (ZIP)")}</button>
      <button class="btn-sm" onclick={resetOnboarding} title={t("Сбросит флаг onboarding_complete и покажет онбординг заново")}>{t("Сбросить онбординг")}</button>
      {#if backupMsg}
        <span class="muted" style="font-size:12px;">{backupMsg}</span>
      {/if}
    </div>
    <div class="preset-row" style="margin-top:8px;">
      <button class="btn-sm" onclick={exportNotesMd}>{t("Экспорт заметок (.md)")}</button>
      <button class="btn-sm" onclick={importNotesMd}>{t("Импорт заметок из папки")}</button>
      {#if notesMdMsg}
        <span class="muted" style="font-size:12px;">{notesMdMsg}</span>
      {/if}
    </div>
    <label class="field" style="margin-top:12px;max-width:280px;">
      <span class="label">{t("Авто-очистка истории (мес., 0 — выкл)")}</span>
      <input type="number" min="0" bind:value={settings.history_cleanup_months} />
    </label>
 <p class="hint">{t("Выполненные задачи старше указанного срока автоматически переносятся в Корзину (не удаляются насовсем — статистика дашборда не страдает, т.к. дата выполнения не стирается). Проверяется раз в сутки.")}</p>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[8] === false} class:hidden-by-tab={SECTION_TAB[8] !== activeTab} bind:this={sectionEls[8]}>
    <h3 class="section-title">{t("Хоткеи")}</h3>

    <!-- v0.9.35: глобальные — отдельной группой над локальными. Порядок не
         косметика: они перехватывают клавиши раньше всего остального, поэтому
         конфликт с ними объясняет, почему «перестал работать» локальный. -->
    <h4 class="keybind-group">{t("Глобальные — работают, даже когда окно закрыто")}</h4>
    <div class="keybind-list">
      {#each globalActions as action (action.id)}
        <div class="keybind-row">
          <span class="keybind-label">{t(action.label)}</span>
          {#if recordingGlobalId === action.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="keybind-combo recording"
              type="text"
              readonly
              value={t("Нажмите комбинацию… (Esc — отмена)")}
              onkeydown={onGlobalCapture}
              autofocus
            />
          {:else}
            <button type="button" class="keybind-combo" onclick={() => startRecordingGlobal(action.id)}>
              {formatCombo(globalComboFor(action.id))}
            </button>
          {/if}
          {#if globalBinds[action.id] && globalBinds[action.id] !== action.default_combo}
            <button type="button" class="btn-icon" title={t("Сбросить к дефолту")} onclick={() => resetGlobalKeybind(action.id)}>↺</button>
          {/if}
        </div>
        {#if globalError?.actionId === action.id}
          <p class="hint" style="color:var(--danger, #d33);margin:0 0 4px 0;">
            {globalError.text}
          </p>
        {/if}
      {/each}
    </div>
    {#if globalFailed.length > 0}
      <!-- Не ошибка ввода, а факт среды: комбинацию уже держит кто-то другой.
           Молчать нельзя — хоткей просто не сработает, и это выглядит как
           поломка приложения. -->
      <p class="hint" style="color:var(--danger, #d33);">
        {t("Система не отдала эти комбинации (заняты другим приложением):")}
        {globalFailed.map(formatCombo).join(", ")}. {t("Выберите другие.")}
      </p>
    {/if}
 <p class="hint">{t("На Wayland (Hyprland, Sway) глобальные хоткеи перехватывает композитор — там их задают в его конфиге, биндом на запуск приложения с")}<code>--quick-task</code>, <code>--quick-note</code>,
      <code>--quick-clip</code>{t("или")}<code>--quick-pinned</code>.
    </p>

    <h4 class="keybind-group">{t("В приложении")}</h4>
    <div class="keybind-list">
      {#each KEYBIND_ACTIONS as action (action.id)}
        <div class="keybind-row">
          <span class="keybind-label">{t(action.label)}</span>
          {#if recordingActionId === action.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="keybind-combo recording"
              type="text"
              readonly
              value={t("Нажмите комбинацию… (Esc — отмена)")}
              onkeydown={onKeybindCapture}
              autofocus
            />
          {:else}
            <button type="button" class="keybind-combo" onclick={() => startRecording(action.id)}>
              {formatCombo(comboFor(keybinds, action.id))}
            </button>
          {/if}
          {#if keybinds[action.id] && keybinds[action.id] !== action.defaultCombo}
            <button type="button" class="btn-icon" title={t("Сбросить к дефолту")} onclick={() => resetKeybind(action.id)}>↺</button>
          {/if}
        </div>
        {#if keybindConflict?.actionId === action.id}
          <p class="hint" style="color:var(--danger, #d33);margin:0 0 4px 0;">
            {t("Конфликт: уже занято действием «{label}» — выберите другую комбинацию.", { label: keybindConflict.withLabel })}
          </p>
        {/if}
      {/each}
    </div>
  </section>

  <section class="card panel" class:hidden-by-search={sectionMatches[9] === false} class:hidden-by-tab={SECTION_TAB[9] !== activeTab} bind:this={sectionEls[9]}>
    <h3 class="section-title">{t("Статусы задач")}</h3>
    {#each statusStore.statuses as s (s.id)}
      <div class="rule-row">
        <input
          type="color"
          class="cat-color"
          value={s.color}
          title={t("Цвет статуса")}
          onchange={(e) => statusStore.update(s.id, { color: e.currentTarget.value })}
        />
        <input
          value={statusStore.name(s.id)}
          disabled={s.is_reserved}
          title={s.is_reserved ? t("Встроенный статус — название нельзя менять") : ""}
          onchange={(e) => {
            const name = e.currentTarget.value.trim();
            if (name && name !== s.name) statusStore.update(s.id, { name });
            else e.currentTarget.value = s.name;
          }}
        />
        {#if !s.is_reserved}
          <button class="btn-icon btn-danger" title={t("Удалить (задачи перейдут в «Todo»)")}
            onclick={() => statusStore.remove(s.id)}>✕</button>
        {:else}
          <span class="hint" style="margin:0;">{t("встроенный")}</span>
        {/if}
      </div>
    {/each}
    <div class="rule-row">
      <input type="color" class="cat-color" bind:value={newStatusColor} title={t("Цвет нового статуса")} />
      <input bind:value={newStatusName} placeholder={t("Новый статус (для канбана)")}
        onkeydown={(e) => { if (e.key === "Enter") addStatus(); }} />
      <button class="btn-sm" onclick={addStatus} disabled={!newStatusName.trim()}>{t("Добавить")}</button>
    </div>
    {#if statusStore.error}
      <p class="hint" style="color:var(--danger, #d33);">{statusStore.error}</p>
    {/if}
 <p class="hint">{t("Изменения сохраняются сразу. Todo/В работе/Готово/Архив — встроенные (с ними связаны трекинг времени и завершение задач), их можно только перекрасить. Свои статусы удобны как промежуточные колонки канбан-доски; при удалении такого статуса задачи переходят в «Todo».")}</p>
  </section>

  <!-- Справка (v0.9.29): содержимое — данными в lib/help.ts, здесь только
       рендер. <details> вместо своего аккордеона: свёрнутый текст остаётся в
       DOM, поэтому существующий поиск по настройкам (читает el.textContent)
       находит его без доработок — совпавшие темы просто раскрываются. -->
  <section class="card panel" class:hidden-by-search={sectionMatches[10] === false} class:hidden-by-tab={SECTION_TAB[10] !== activeTab} bind:this={sectionEls[10]}>
    <h3 class="section-title">{t("Справка")}</h3>
 <p class="hint" style="margin-top:0;">{t("Что умеет приложение. Раскройте тему, чтобы прочитать; поиск по настройкам ищет и здесь.")}</p>
    {#each HELP_TOPICS as topic (topic.id)}
      <details class="help-topic" open={helpSearchOpen}>
        <!-- Переводится при отрисовке, а не в help.ts: справка — чистые
             данные без рун, и держать её словарём удобнее одним местом. -->
        <summary>{t(topic.title)}</summary>
        <dl class="help-list">
          {#each topic.items as item (item.term)}
            <dt>{t(item.term)}</dt>
            <dd>{t(item.desc)}</dd>
          {/each}
        </dl>
      </details>
    {/each}
  </section>

  <button class="btn-primary" onclick={save} disabled={saving}>
    {saving ? t("Сохранение...") : saved ? t("Сохранено ✓") : t("Сохранить")}
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

  .hidden-by-search, .hidden-by-tab {
    display: none;
  }

  .settings-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 14px;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--border);
  }

  .settings-tab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 8px 12px;
    font-size: 13px;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .settings-tab:hover {
    color: var(--text-primary);
  }

  .settings-tab.active {
    color: var(--text-primary);
    border-bottom-color: var(--accent);
    font-weight: 500;
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

  /* Справка (v0.9.29) */
  .help-topic {
    border-top: 1px solid var(--border);
    padding: 8px 0;
  }

  .help-topic summary {
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
    list-style: none;
  }

  /* Своя стрелка вместо дефолтного маркера — он рисуется по-разному
     в разных движках (тот же принцип, что с иконками в Icon.svelte). */
  .help-topic summary::marker,
  .help-topic summary::-webkit-details-marker { display: none; }

  .help-topic summary::before {
    content: "▸";
    display: inline-block;
    width: 14px;
    color: var(--text-secondary);
  }

  .help-topic[open] summary::before { content: "▾"; }

  .help-list {
    margin: 8px 0 4px 14px;
    font-size: 12px;
  }

  .help-list dt {
    font-weight: 600;
    margin-top: 8px;
  }

  .help-list dd {
    margin: 2px 0 0 0;
    color: var(--text-secondary);
    line-height: 1.45;
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

  .keybind-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* v0.9.35: заголовок группы внутри секции — глобальные и локальные хоткеи
     живут на одной вкладке, но это разные механизмы, и их нельзя читать
     одним сплошным списком. */
  .keybind-group {
    margin: 12px 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted, #888);
  }
  .keybind-group:first-of-type {
    margin-top: 0;
  }

  .keybind-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .keybind-label {
    flex: 1;
    font-size: 13px;
  }

  .keybind-combo {
    font-size: 12px;
    font-family: inherit;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
    min-width: 120px;
    text-align: center;
    cursor: pointer;
  }

  .keybind-combo.recording {
    border-color: var(--accent);
    color: var(--text-secondary);
    cursor: default;
    min-width: 220px;
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
