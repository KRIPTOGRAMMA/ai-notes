<script lang="ts">
  import { enable as enableAutostart, disable as disableAutostart } from "@tauri-apps/plugin-autostart";
  import { api } from "../lib/api/tauri";
  import type { AppSettings } from "../lib/types";
  import ModelDownloader from "../lib/components/ModelDownloader.svelte";

  interface Props {
    settings: AppSettings;
    isWayland: boolean;
    onDone: () => void;
  }
  let { settings, isWayland, onDone }: Props = $props();

  // Шаг 3 (Wayland) показываем только на Wayland
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

<div style="max-width:480px;margin:48px auto;padding:24px;border:1px solid var(--border,#e5e7eb);border-radius:12px;">
  <div style="font-size:12px;color:var(--text-secondary,#6b7280);margin-bottom:16px;">
    Шаг {stepIdx + 1} из {steps.length}
  </div>

  {#if error}
    <div style="background:#fee2e2;color:#dc2626;padding:8px 12px;border-radius:6px;margin-bottom:12px;">{error}</div>
  {/if}

  {#if step === 1}
    <h2 style="margin-top:0;">Добро пожаловать в AI Notes</h2>
    <p>Задачи, заметки и мониторинг активности — всё локально, приватно и с опциональным ИИ.</p>
    <p style="color:var(--text-secondary,#6b7280);font-size:14px;">Пара минут настройки — и можно работать.</p>
  {:else if step === 2}
    <h2 style="margin-top:0;">ИИ-помощник</h2>
    <p style="font-size:14px;">ИИ переписывает задачи в SMART-формат, генерирует подзадачи и классифицирует их.</p>
    <div style="display:flex;flex-direction:column;gap:8px;">
      <label style="display:flex;gap:8px;align-items:flex-start;cursor:pointer;">
        <input type="radio" name="ai" value="local" bind:group={aiChoice} />
        <span><b>Локальная модель</b><br/>
          <small style="color:var(--text-secondary,#6b7280);">Приватно, работает оффлайн. GGUF-модель можно скачать прямо здесь.</small></span>
      </label>
      {#if aiChoice === "local"}
        <div style="margin:4px 0 4px 26px;">
          <ModelDownloader />
        </div>
      {/if}
      <label style="display:flex;gap:8px;align-items:flex-start;cursor:pointer;">
        <input type="radio" name="ai" value="cloud" bind:group={aiChoice} />
        <span><b>Облачный API</b><br/>
          <small style="color:var(--text-secondary,#6b7280);">OpenAI или Anthropic — API-ключ вводится в Настройках</small></span>
      </label>
      <label style="display:flex;gap:8px;align-items:flex-start;cursor:pointer;">
        <input type="radio" name="ai" value="none" bind:group={aiChoice} />
        <span><b>Без ИИ</b><br/>
          <small style="color:var(--text-secondary,#6b7280);">Можно включить позже в Настройках</small></span>
      </label>
    </div>
  {:else if step === 3}
    <h2 style="margin-top:0;">Мониторинг на Wayland</h2>
    <p style="font-size:14px;">По умолчанию активность отслеживается, только когда окно приложения в фокусе.</p>
    <p style="font-size:14px;">Для полного трекинга в фоне добавь свой аккаунт в группу <code>input</code>:</p>
    <pre style="background:var(--bg-secondary,#f3f4f6);padding:8px 12px;border-radius:6px;font-size:13px;">sudo usermod -aG input $USER</pre>
    <p style="font-size:13px;color:var(--text-secondary,#6b7280);">Потребуется перелогин. Этот шаг можно пропустить и вернуться к нему позже.</p>
  {:else if step === 4}
    <h2 style="margin-top:0;">Автозагрузка и хоткеи</h2>
    <label style="display:flex;gap:8px;align-items:center;cursor:pointer;margin-bottom:12px;">
      <input type="checkbox" bind:checked={autostart} />
      Запускать AI Notes при входе в систему
    </label>
    <p style="font-size:14px;">Быстрая задача из любого места: <b>Ctrl+Shift+N</b></p>
    <p style="font-size:13px;color:var(--text-secondary,#6b7280);">
      На Hyprland/Sway глобальные хоткеи перехватывает композитор — добавь бинд, запускающий
      <code>ai-notes --quick-task</code>.
    </p>
  {:else}
    <h2 style="margin-top:0;">Готово!</h2>
    <ul style="font-size:14px;padding-left:18px;">
      <li><b>Задачи</b> — создание через кнопку или Ctrl+Shift+N</li>
      <li><b>Дашборд</b> — активность и выполненные задачи по дням</li>
      <li><b>Трей</b> — быстрое переключение режима (Focus — без уведомлений, Study — помодоро)</li>
    </ul>
  {/if}

  <div style="display:flex;gap:8px;margin-top:24px;">
    {#if stepIdx > 0}
      <button onclick={back}>Назад</button>
    {/if}
    <span style="flex:1;"></span>
    {#if stepIdx < steps.length - 1}
      <button onclick={next}>{step === 1 ? "Начать настройку" : "Далее"}</button>
    {:else}
      <button onclick={finish} disabled={finishing}>{finishing ? "Сохранение..." : "Начать"}</button>
    {/if}
  </div>
</div>
