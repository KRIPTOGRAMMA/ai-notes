<script lang="ts">
  import { enable as enableAutostart, disable as disableAutostart } from "@tauri-apps/plugin-autostart";
  import { api } from "../lib/api/tauri";
  import type { AppSettings } from "../lib/types";
  import ModelDownloader from "../lib/components/ModelDownloader.svelte";

  import { t } from "../lib/i18n.svelte";
  interface Props {
    settings: AppSettings;
    isWayland: boolean;
    onDone: () => void;
  }
  let { settings, isWayland, onDone }: Props = $props();

  // Step 3 (Wayland) is shown on Wayland only
  const steps = isWayland ? [1, 2, 3, 4, 5] : [1, 2, 4, 5];
  let stepIdx = $state(0);
  let step = $derived(steps[stepIdx]);

  let aiChoice = $state<"local" | "cloud" | "none">("none");
  let autostart = $state(false);
  let error: string | null = $state(null);
  let finishing = $state(false);

  function next() {
    if (stepIdx < steps.length - 1) stepIdx += 1;
  }
  function back() {
    if (stepIdx > 0) stepIdx -= 1;
  }

  async function finish() {
    finishing = true;
    error = null;
    try {
      if (autostart) {
        await enableAutostart();
      } else {
        await disableAutostart().catch(() => {});
      }
      settings.ai_provider =
        aiChoice === "cloud" ? "openai" : aiChoice === "none" ? "none" : "local";
      settings.onboarding_complete = true;
      await api.saveSettings(settings);
      onDone();
    } catch (e) {
      error = String(e);
    } finally {
      finishing = false;
    }
  }
</script>

<div class="wrap">
  <div class="card box">
    <div class="progress">
      <span class="muted" style="font-size:12px;">{t("Шаг {i} из {n}", { i: stepIdx + 1, n: steps.length })}</span>
      <div class="steps-track">
        {#each steps as _, i}
          <span class="step-dot" class:done={i <= stepIdx}></span>
        {/each}
      </div>
    </div>

    {#if error}
      <div class="alert">{error}</div>
    {/if}

    {#if step === 1}
      <h2>{t("Добро пожаловать в AI Notes")}</h2>
      <p>{t("Задачи, заметки и мониторинг активности — всё локально, приватно и с опциональным ИИ.")}</p>
      <p class="muted">{t("Пара минут настройки — и можно работать.")}</p>
    {:else if step === 2}
      <h2>{t("ИИ-помощник")}</h2>
      <p>{t("ИИ переписывает задачи в SMART-формат, генерирует подзадачи и классифицирует их.")}</p>
      <div class="options">
        <label class="option">
          <input type="radio" name="ai" value="local" bind:group={aiChoice} />
          <span><b>{t("Локальная модель")}</b><br/>
            <small class="muted">{t("Приватно, работает оффлайн. GGUF-модель можно скачать прямо здесь.")}</small></span>
        </label>
        {#if aiChoice === "local"}
          <div style="margin:4px 0 4px 26px;">
            <ModelDownloader />
          </div>
        {/if}
        <label class="option">
          <input type="radio" name="ai" value="cloud" bind:group={aiChoice} />
          <span><b>{t("Облачный API")}</b><br/>
            <small class="muted">{t("OpenAI или Anthropic — API-ключ вводится в Настройках")}</small></span>
        </label>
        <label class="option">
          <input type="radio" name="ai" value="none" bind:group={aiChoice} />
          <span><b>{t("Без ИИ")}</b><br/>
            <small class="muted">{t("Можно включить позже в Настройках")}</small></span>
        </label>
      </div>
    {:else if step === 3}
      <h2>{t("Мониторинг на Wayland")}</h2>
      <p>
        {t("Активность отслеживается системно: композитор сам сообщает о простое и возврате (протокол")} <code>ext-idle-notify</code>{t("). Настраивать ничего не нужно, содержимое ввода приложению не видно — только факт активности.")}
      </p>
      <p class="muted" style="font-size:13px;">
        {t("Если композитор не поддерживает протокол, трекинг работает только при окне в фокусе. Текущий режим виден в Настройках → Мониторинг.")}
      </p>
    {:else if step === 4}
      <h2>{t("Автозагрузка и хоткеи")}</h2>
      <label class="option" style="margin-bottom:12px;align-items:center;">
        <input type="checkbox" bind:checked={autostart} />
        {t("Запускать AI Notes при входе в систему")}
      </label>
      <p>{t("Быстрая задача из любого места:")} <kbd>Ctrl Shift N</kbd></p>
      <!-- The compositor tip, on Wayland only. It used to be shown to everyone,
           including on Windows, where neither Hyprland/Sway nor a bind in a
           compositor config means anything. -->
      {#if isWayland}
        <p class="muted" style="font-size:13px;">
          {t("На Hyprland/Sway глобальные хоткеи перехватывает композитор — добавь бинд, запускающий")}
          <code>ai-notes --quick-task</code>.
        </p>
      {/if}
    {:else}
      <h2>{t("Готово!")}</h2>
      <ul>
        <li><b>{t("Задачи")}</b> {t("— создание через кнопку или")} <kbd>Ctrl Shift N</kbd></li>
        <li><b>{t("Дашборд")}</b> {t("— активность и выполненные задачи по дням")}</li>
        <li><b>{t("Трей")}</b> {t("— быстрое переключение режима (Focus — без уведомлений, Study — помодоро)")}</li>
      </ul>
      <!-- The onboarding deliberately stays short: the full tour of the features
           lives in one place, Settings -> Help. Duplicating it here would mean
           two texts that diverge at the first edit. -->
      <p class="muted" style="font-size:13px;margin-top:10px;">
        {t("Остальное — в")} <b>{t("Настройках → Справка")}</b>{t(": там собрано, что умеют заметки, задачи, быстрый ввод, ИИ и мониторинг.")}
      </p>
    {/if}

    <div class="actions">
      {#if stepIdx > 0}
        <button class="btn-ghost" onclick={back}>{t("Назад")}</button>
      {/if}
      <span style="flex:1;"></span>
      {#if stepIdx < steps.length - 1}
        <button class="btn-primary" onclick={next}>{step === 1 ? t("Начать настройку") : t("Далее")}</button>
      {:else}
        <button class="btn-primary" onclick={finish} disabled={finishing}>{finishing ? t("Сохранение...") : t("Начать")}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .wrap {
    height: 100vh;
    overflow-y: auto;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 48px 16px;
  }

  .box {
    width: 100%;
    max-width: 480px;
    padding: 22px 24px;
  }

  .progress {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
  }

  .steps-track {
    display: flex;
    gap: 4px;
  }

  .step-dot {
    width: 18px;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-hover);
  }

  .step-dot.done {
    background: var(--accent);
  }

  h2 {
    margin: 0 0 10px 0;
    font-size: 17px;
  }

  p { margin: 0 0 10px 0; font-size: 13px; }

  ul {
    font-size: 13px;
    padding-left: 18px;
    margin: 0;
  }

  ul li { margin: 4px 0; }

  .options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .option {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    cursor: pointer;
    font-size: 13px;
  }

  pre {
    background: var(--bg-secondary);
    padding: 8px 12px;
    border-radius: var(--radius);
    font-size: 12px;
    overflow-x: auto;
  }

  code {
    background: var(--bg-secondary);
    padding: 1px 4px;
    border-radius: 4px;
    font-size: 0.95em;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 20px;
  }
</style>
