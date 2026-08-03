<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  import { i18n, t, tErr } from "../lib/i18n.svelte";
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

  // Each preset sets a pair of accents (primary plus secondary, the .btn-primary
  // gradient) with one button; "Custom" leaves the manual pickers below untouched.
  const THEME_PRESETS: { name: string; accent: string; accentSecondary: string }[] = $derived([
    { name: "Indigo", accent: "#6366f1", accentSecondary: "#6366f1" },
    { name: t("Океан"), accent: "#0891b2", accentSecondary: "#6366f1" },
    { name: t("Закат"), accent: "#f43f5e", accentSecondary: "#f59e0b" },
    { name: t("Лес"), accent: "#10b981", accentSecondary: "#65a30d" },
    { name: "Rose", accent: "#f43f5e", accentSecondary: "#f43f5e" },
    { name: "Slate", accent: "#64748b", accentSecondary: "#64748b" },
  ]);

  // The theme is applied on every change — a live preview without pressing "Save".
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
  let whisperPath: string | null = $state(null);
  // The number of cleared domain records, shown after the click so the action does
  // not look like "nothing happened".
  let domainCleared: number | null = $state(null);

  async function clearDomains() {
    domainCleared = await api.clearDomainHistory().catch(() => null);
  }

  // --- Tabs: the sections are grouped so there is no single long column to scroll.
  // SECTION_TAB[i] is the tab id for the section at index i (the section indices are
  // the same ones sectionEls/sectionMatches use below). The labels go through a
  // $derived rather than a plain const: the language changes without a reload, and a
  // const would be computed once at module load and leave the tabs in the old one.
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
  // Appearance(0) and Work mode(2) -> General; AI provider(1) -> AI;
  // Monitoring(3) and Task categories(4) -> Tasks; Notifications(5) ->
  // Notifications; Auto-backup(6) and Data(7) -> Data; Hotkeys(8) -> Hotkeys;
  // Statuses(9) -> Tasks (appended last by index so the existing sections did not
  // have to be renumbered, but logically grouped with Categories); Help(10) -> Help
  // (also appended last by index for the same reason); Voice input(11) -> AI (same
  // again: appended by index, grouped with the AI provider).
  const SECTION_TAB: TabId[] = ["general", "ai", "general", "tasks", "tasks", "notifications", "data", "data", "hotkeys", "tasks", "help", "ai"];
  let activeTab = $state<TabId>("general");

  // --- Settings search: a plain substring match over the whole text of a section,
  // with no indexing or fuzziness. An empty query shows everything everywhere; a
  // non-empty one automatically switches to the first tab with a match, and inside
  // that tab non-matching sections stay hidden.
  let searchQuery = $state("");
  let sectionEls: HTMLElement[] = $state([]);
  let sectionMatches = $state<boolean[]>([]);
  // While a search is active the help topics are expanded: otherwise the match sits
  // inside a collapsed <details> and the user sees a topic with no visible text.
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

  // "Window class -> category" rules: edited as rows and serialized into
  // settings.app_category_rules on save.
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

  // --- AI suggestion of app rules ---
  //
  // Apps with no rule all land in "Other", and writing globs by hand is exactly the
  // work worth handing to a model. Suggest-then-confirm, as everywhere else here:
  // the model only proposes and the rules appear in the list by an explicit click.
  //
  // Writing them straight into the settings would silently rewrite statistics for
  // past days — the categories are applied at read time, so a wrong rule would
  // retroactively distort the dashboard.
  let ruleSuggestBusy = $state(false);
  let ruleSuggestError = $state("");
  // Suggestions with a checkbox each. Ticked by default: the usual case is accepting
  // nearly everything rather than picking items one by one.
  let ruleSuggestions: { pattern: string; category: string; take: boolean }[] = $state([]);
  // Shown when the model returned nothing to add — a distinct state from "not run
  // yet", or the button would look broken.
  let ruleSuggestEmpty = $state(false);

  async function suggestAppRules() {
    ruleSuggestBusy = true;
    ruleSuggestError = "";
    ruleSuggestEmpty = false;
    ruleSuggestions = [];
    try {
      await api.aiSuggestAppRules();
    } catch (e) {
      ruleSuggestBusy = false;
      ruleSuggestError = String(e);
    }
  }

  // Accepted rules go to the START of the list: the first match wins in
  // categorize_app, and appended at the end they would be shadowed by a broader
  // user-written pattern (say "*fox") and quietly do nothing.
  function acceptRuleSuggestions() {
    const picked = ruleSuggestions.filter(r => r.take);
    appRules = [...picked.map(r => ({ pattern: r.pattern, category: r.category })), ...appRules];
    ruleSuggestions = [];
  }

  // Time limits per app category: one entry per category, where 0 or empty means no
  // limit. Serialized into settings.app_limits on save.
  let appLimits: Record<string, number> = $state({});

  function parseLimits(json: string): AppLimit[] {
    try {
      const v = JSON.parse(json);
      return Array.isArray(v) ? v : [];
    } catch {
      return [];
    }
  }

  // The model's answer arrives as an event, like every other AI command here.
  let ruleUnlisten: UnlistenFn | null = null;
  onDestroy(() => ruleUnlisten?.());

  onMount(async () => {
    ruleUnlisten = await listen<{ rules: AppCategoryRule[] | null; error: string | null }>(
      "ai-app-rules",
      (e) => {
        ruleSuggestBusy = false;
        if (e.payload.error) {
          ruleSuggestError = e.payload.error;
          return;
        }
        const proposed = e.payload.rules ?? [];
        // Anything already present in the list is dropped: the model may repeat a
        // rule the user added while it was thinking.
        const fresh = proposed.filter(
          p => !appRules.some(r => r.pattern.trim().toLowerCase() === p.pattern.toLowerCase()),
        );
        ruleSuggestions = fresh.map(r => ({ ...r, take: true }));
        ruleSuggestEmpty = fresh.length === 0;
      },
    );
    try {
      settings = await api.getSettings();
      // An empty setting means the language was never chosen explicitly. The select
      // shows the language actually in effect (determined by i18n.init from the
      // locale), otherwise the field would look empty while the translation works.
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
    // The list of global actions comes from the backend, which is what registers them.
    globalActions = await api.listGlobalActions().catch(() => []);
    trackingMode = await api.getTrackingMode().catch(() => null);
    windowTracking = await api.getWindowTracking().catch(() => null);
    // The real path from the backend rather than a string assembled on the
    // frontend: the directory depends on the OS (app_data_dir) and on the
    // application's identifier.
    modelPath = await api.modelPath().catch(() => null);
    whisperPath = await api.modelPath("whisper").catch(() => null);
    categoryStore.load();
    statusStore.load();
  });

  // --- Hotkeys: the overrides live in settings.keybinds (JSON) and the defaults in
  // KEYBIND_ACTIONS.defaultCombo. A new binding is recorded by clicking "Record",
  // and the next key press that is not a modifier is captured.
  let keybinds: Keybinds = $state({});
  let recordingActionId: string | null = $state(null);
  let keybindConflict: { actionId: string; withLabel: string } | null = $state(null);

  // While recording, App.svelte does not execute hotkeys: otherwise recording a
  // combination already taken by a local action (Ctrl+K) would run that action and
  // pull focus out of the recording field.
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
    if (!combo) return; // only a modifier was pressed — we wait for the main key

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

  // --- Global hotkeys ---
  //
  // The combination format matches the webview hotkeys ("Ctrl+Shift+KeyN"): it was
  // specifically verified that the global-hotkey parser understands both that form
  // and "Ctrl+Shift+N", so no converter between formats is needed and recording a
  // combination in the UI is identical for both groups.
  //
  // There are three differences from the local ones: the action list comes from the
  // backend (which also registers them), the backend validates the combination, and
  // after saving a re-registration is required — otherwise a new combination would
  // only start working after an application restart.
  let globalActions: GlobalAction[] = $state([]);
  let globalBinds: Keybinds = $state({});
  let recordingGlobalId: string | null = $state(null);
  let globalError: { actionId: string; text: string } | null = $state(null);
  // Combinations the OS refused to hand over (taken by another application or by
  // the compositor). Shown separately: this is not an input error but a fact about
  // the environment.
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
    if (!combo) return; // only a modifier — we wait for the main key

    const actionId = recordingGlobalId;

    // A conflict within the group: the OS cannot tell two global commands on one
    // combination apart.
    const dupe = globalActions.find(a => a.id !== actionId && globalComboFor(a.id) === combo);
    if (dupe) {
      globalError = { actionId, text: t("Уже занято: {label}", { label: dupe.label }) };
      return;
    }
    // A conflict with a local hotkey: the global one intercepts keys first, so the
    // local one would simply stop working — silently and inexplicably.
    const localDupe = KEYBIND_ACTIONS.find(a => comboFor(keybinds, a.id) === combo);
    if (localDupe) {
      globalError = { actionId, text: t("Занято хоткеем в приложении: {label}", { label: localDupe.label }) };
      return;
    }
    // The final say belongs to the real combination parser rather than to our own
    // rules: it is the one that will do the registering.
    try {
      await api.validateGlobalCombo(combo);
    } catch (err) {
      // While the answer was awaited the user may have left recording (Escape) or
      // started recording another action, in which case the reply is stale and its
      // error must not be shown: doing so would put the field back into recording mode.
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

  // --- Task categories (CRUD is saved immediately, with no "Save" button) ---
  let newCatName = $state("");
  let newCatColor = $state("#2a78d6");

  async function addCategory() {
    const name = newCatName.trim();
    if (!name) return;
    await categoryStore.create(name, newCatColor);
    newCatName = "";
  }

  // --- Task statuses (for the kanban board), following the same pattern as
  // categories: CRUD is saved immediately, with no "Save" button.
  // Todo/InProgress/Done/Archived are reserved (is_reserved) and can be neither
  // renamed nor deleted.
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
      // Re-registration with the OS: without it a new combination would only start
      // working after a restart while the old one kept firing.
      globalFailed = await api.applyGlobalHotkeys().catch(() => []);
      applyTheme(settings.theme_mode, settings);
      // App.svelte keeps its own copy of the hotkeys for the keydown handler —
      // without this event a rebinding would only take effect after a reload.
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

  // A test button: reset the onboarding and reload the webview so App.svelte
  // re-reads the settings and shows the onboarding straight away. We take fresh
  // settings from the DB so unsaved form edits are not written along with it.
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

  <!-- seg + seg--tabs provide the look; .settings-tabs remains for its own
       spacing and its wrap onto a second line. The .settings-tab name is left
       alone: about 25 e2e tests select by it, and this change is purely
       cosmetic. -->
  <div class="settings-tabs seg seg--tabs" role="tablist">
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

    <!-- Language: applied immediately, without "Save", just like the theme. For
         the language that matters more than for the theme: seeing the result
         before saving is the only way to tell you picked the right one. -->
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

    <!-- One settings block whose fields depend on the chosen provider, not two
         parallel duplicating blocks as there were with the radio list. -->
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

    <!-- Domains: shown in the same place window tracking works — without a
         provider there is nowhere to read a title from and the checkbox would be
         dead. The wording is deliberately blunt: the user must understand what
         will actually start happening rather than see an innocuous "improve the
         statistics". -->
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

      <!-- Suggest-then-confirm: the model proposes, the rules appear in the list
           above only by an explicit click. Hidden when AI is off — the same
           capability detection as the other AI buttons. -->
      {#if settings.ai_provider !== "none"}
        <button class="btn-sm" style="margin-top:6px;" onclick={suggestAppRules} disabled={ruleSuggestBusy}>
          {ruleSuggestBusy ? t("Определяю…") : t("Определить категории через ИИ")}
        </button>
        {#if ruleSuggestError}
          <span class="alert" style="margin-top:6px;">{ruleSuggestError}</span>
        {/if}
        {#if ruleSuggestEmpty}
          <p class="hint">{t("Все приложения из статистики уже покрыты правилами.")}</p>
        {/if}
        {#if ruleSuggestions.length > 0}
          <div class="rule-suggestions">
            {#each ruleSuggestions as sug (sug.pattern)}
              <label class="rule-row suggestion-row">
                <input type="checkbox" bind:checked={sug.take} />
                <code style="flex:1;">{sug.pattern}</code>
                <span class="muted">{RULE_CATEGORIES.find(c => c.value === sug.category)?.label ?? sug.category}</span>
              </label>
            {/each}
            <button class="btn-sm" onclick={acceptRuleSuggestions}
              disabled={!ruleSuggestions.some(r => r.take)}>
              {t("Добавить отмеченные")}
            </button>
          </div>
        {/if}
      {/if}

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
          We show the translated name, but that makes the seeded categories
          uneditable: the field is bound to the same value that goes into the DB,
          and the translation would overwrite the Russian original for good. The
          same approach the statuses below take with is_reserved. Categories have
          no such flag; a seeded one is recognized by its Latin id (user-defined
          ones get a uuid).
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
      <p class="hint" style="color:var(--danger, #d33);">{tErr(categoryStore.error)}</p>
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

    <!-- The global ones form a separate group above the local ones. The order is
         not cosmetic: they intercept keys before anything else, so a conflict
         with them explains why a local hotkey "stopped working". -->
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
      <!-- Not an input error but a fact about the environment: someone else already
           holds the combination. Staying silent is not an option — the hotkey
           simply will not fire, and that looks like a broken application. -->
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
      <p class="hint" style="color:var(--danger, #d33);">{tErr(statusStore.error)}</p>
    {/if}
 <p class="hint">{t("Изменения сохраняются сразу. Todo/В работе/Готово/Архив — встроенные (с ними связаны трекинг времени и завершение задач), их можно только перекрасить. Свои статусы удобны как промежуточные колонки канбан-доски; при удалении такого статуса задачи переходят в «Todo».")}</p>
  </section>

  <!-- Help: the content lives as data in lib/help.ts, this file only renders it.
       <details> instead of a custom accordion: collapsed text stays in the DOM,
       so the existing settings search (which reads el.textContent) finds it with
       no extra work — matching topics simply expand. -->
  <section class="card panel" class:hidden-by-search={sectionMatches[10] === false} class:hidden-by-tab={SECTION_TAB[10] !== activeTab} bind:this={sectionEls[10]}>
    <h3 class="section-title">{t("Справка")}</h3>
 <p class="hint" style="margin-top:0;">{t("Что умеет приложение. Раскройте тему, чтобы прочитать; поиск по настройкам ищет и здесь.")}</p>
    {#each HELP_TOPICS as topic (topic.id)}
      <details class="help-topic" open={helpSearchOpen}>
        <!-- Translated at render time rather than in help.ts: the help is pure data
             with no runes, and keeping it in one dictionary is simpler. -->
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

  <!-- Voice input. A separate section rather than a block inside the AI one because
       it does not depend on ai_provider at all: recognition always runs locally, so
       the model is needed even when the chat model is a cloud one. -->
  <section class="card panel" class:hidden-by-search={sectionMatches[11] === false} class:hidden-by-tab={SECTION_TAB[11] !== activeTab} bind:this={sectionEls[11]}>
    <h3 class="section-title">{t("Голосовой ввод")}</h3>
    <p class="hint" style="margin-top:0;">{t("Распознавание речи работает полностью на этом компьютере: запись никуда не отправляется. Нужна отдельная модель — её можно скачать здесь.")}</p>
    <p class="muted" style="font-size:12px;margin:0 0 10px 0;">{t("Модель распознавания хранится в")}<code>{whisperPath ?? "…"}</code>
    </p>
    <ModelDownloader kind="whisper" />
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

  /* The look comes from the shared .seg; only the layout remains here. There
     are seven tabs and in a narrow window they do not fit on one line, so
     wrapping is mandatory — and with it align-self: flex-start, or the first
     row's segments would stretch to the height of the second. */
  .settings-tabs {
    margin-bottom: 14px;
    flex-wrap: wrap;
    align-self: flex-start;
    max-width: 100%;
  }

  /* A wrapped row would butt right up against the border: .seg has a single
     border for them all, and the dividers only draw the vertical seams. */
  .settings-tabs .settings-tab {
    border-top: 1px solid transparent;
  }

  .settings-tab {
    cursor: pointer;
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

  /* Help */
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

  /* Our own arrow instead of the default marker, which different engines draw
     differently (the same principle as the icons in Icon.svelte). */
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

  /* Suggestions sit apart from the rules themselves: they are not yet part of the
     settings and must not read as rows already in effect. */
  .rule-suggestions {
    margin-top: 6px;
    padding: 8px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .suggestion-row {
    cursor: pointer;
    align-items: center;
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

  /* A group heading inside a section: global and local hotkeys live on one tab,
     but they are different mechanisms and must not read as one continuous
     list. */
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
