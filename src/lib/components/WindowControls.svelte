<script lang="ts">
  // Свои кнопки окна вместо системного заголовка (v0.9.40).
  //
  // Главное окно объявлено с decorations: false, поэтому WebKitGTK больше не
  // рисует белую плашку с названием — вместе с ней исчезают и системные
  // кнопки, и возможность таскать окно мышью. И то, и другое возвращается
  // здесь.
  //
  // Зона перетаскивания — не data-tauri-drag-region, а явный вызов
  // startDragging по mousedown: атрибут срабатывает на любой клик внутри
  // размеченного элемента, включая клики по вложенным кнопкам, и тогда
  // нажатие «свернуть» превращалось бы в микро-перетаскивание вместо
  // клика.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n.svelte";

  let maximized = $state(false);

  // Кнопка «развернуть» должна показывать текущее состояние: разворачивать
  // можно и двойным кликом по шапке, и через WM — иконка обязана следовать
  // за окном, а не за своими же кликами.
  $effect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let alive = true;
    win.isMaximized().then((v) => { if (alive) maximized = v; }).catch(() => {});
    win.onResized(() => {
      win.isMaximized().then((v) => { if (alive) maximized = v; }).catch(() => {});
    }).then((fn) => {
      if (alive) unlisten = fn; else fn();
    }).catch(() => {});
    return () => { alive = false; unlisten?.(); };
  });

  async function startDrag(e: MouseEvent) {
    // Только основная кнопка: правый клик на шапке принадлежит WM.
    if (e.button !== 0) return;
    try {
      await getCurrentWindow().startDragging();
    } catch {
      // Перетаскивание — не та операция, ради которой стоит рушить UI.
    }
  }

  async function toggleMaximize() {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch {
      /* см. выше */
    }
  }

  async function minimize() {
    try {
      await getCurrentWindow().minimize();
    } catch {
      /* см. выше */
    }
  }

  // Закрытие прячет окно, а не завершает процесс: приложение живёт в трее,
  // фоновые циклы (трекинг, помодоро, уведомления) должны продолжать
  // работать. Выход — только из меню трея, как и было с системной кнопкой.
  async function close() {
    try {
      await getCurrentWindow().hide();
    } catch {
      /* см. выше */
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="titlebar" onmousedown={startDrag} ondblclick={toggleMaximize}>
  <div class="controls">
    <button class="win-btn" onclick={minimize} title={t("Свернуть")} aria-label={t("Свернуть")}>
      <Icon name="winmin" size={14} />
    </button>
    <button
      class="win-btn"
      onclick={toggleMaximize}
      title={maximized ? t("Восстановить") : t("Развернуть")}
      aria-label={maximized ? t("Восстановить") : t("Развернуть")}
    >
      <Icon name={maximized ? "collapse" : "winmax"} size={14} />
    </button>
    <button class="win-btn close" onclick={close} title={t("Закрыть")} aria-label={t("Закрыть")}>
      <Icon name="winclose" size={14} />
    </button>
  </div>
</div>

<style>
  /* Плавает над контентом: своей высоты не занимает, чтобы вьюхи не
     пришлось сдвигать вниз на высоту шапки. Тянется на всю ширину — вся
     верхняя кромка окна работает как зона перетаскивания. */
  .titlebar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 32px;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding-right: 6px;
    z-index: 60;
    /* Клики проходят насквозь везде, кроме самих кнопок и зоны справа:
       иначе шапка перехватывала бы верхнюю строку контента. */
    pointer-events: none;
  }

  .controls {
    display: flex;
    gap: 2px;
    pointer-events: auto;
  }

  .win-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-secondary);
  }

  .win-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .win-btn.close:hover {
    background: var(--danger);
    color: #fff;
  }
</style>
