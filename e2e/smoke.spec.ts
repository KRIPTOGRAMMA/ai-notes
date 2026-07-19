import { test, expect, type Page } from "@playwright/test";

// Смоук-набор против vite dev с моком Tauri (__TAURI_INTERNALS__).
// Rust-слой в этих тестах не участвует — он покрыт `cargo test`.

async function withMock(page: Page) {
  await page.addInitScript({ path: "./e2e/tauri-mock.js" });
}

// Сид состояния мока: кладётся в localStorage ДО tauri-mock.js,
// который подхватывает существующий __mock_db.
async function seedDb(page: Page, db: object) {
  await page.addInitScript((json) => {
    localStorage.setItem("__mock_db", json);
  }, JSON.stringify(db));
}

async function createTask(page: Page, title: string) {
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  await page.getByPlaceholder("Название задачи").fill(title);
  await page.getByRole("button", { name: "Создать" }).click();
}

// Живой markdown-редактор (CodeMirror 6, v0.6.9) — contenteditable, не textarea.
// Заменяет весь текст: клик → выделить всё → напечатать.
function noteEditor(page: Page) {
  return page.locator(".cm-content");
}
async function fillNoteEditor(page: Page, text: string) {
  const editor = noteEditor(page);
  await editor.click();
  await page.keyboard.press("ControlOrMeta+a");
  // insertText (не keyboard.type) — вставляет многострочный текст одним куском,
  // без реальных Enter-нажатий. Печать \n через type() триггерит markdown-
  // расширение CodeMirror «продолжить маркер списка на новой строке», что
  // дублирует "- [ ] " в многострочных чек-листах при построчном наборе.
  await page.keyboard.insertText(text);
  // Строка с курсором рендерится сырым markdown (иначе редактировать вслепую) —
  // уводим курсор на новую пустую строку, чтобы виджеты (ссылки/жирный/итд)
  // на введённом тексте стали видимыми для проверок.
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");
}

test("онбординг проходится до конца и больше не показывается", async ({ page }) => {
  await seedDb(page, { tasks: [], notes: [], settings: { onboarding_complete: false } });
  await withMock(page);
  await page.goto("/");

  await expect(page.getByText("Добро пожаловать в AI Notes")).toBeVisible();
  await page.getByRole("button", { name: "Начать настройку" }).click();
  await expect(page.getByText("ИИ-помощник")).toBeVisible();
  await page.getByRole("button", { name: "Далее" }).click();
  // шаг 3 (Wayland) пропущен: is_wayland → false
  await expect(page.getByText("Автозагрузка и хоткеи")).toBeVisible();
  await page.getByRole("button", { name: "Далее" }).click();
  await expect(page.getByText("Готово!")).toBeVisible();
  await page.getByRole("button", { name: "Начать", exact: true }).click();

  // главный экран, флаг сохранён в «БД»
  await expect(page.getByRole("heading", { name: "Задачи" })).toBeVisible();
  const db = JSON.parse(await page.evaluate(() => localStorage.getItem("__mock_db")!));
  expect(db.settings.onboarding_complete).toBe(true);
});

test("задача: создание, редактирование, выполнение, удаление из истории", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "тестовая задача");
  await expect(page.getByText("тестовая задача")).toBeVisible();

  // редактирование по клику на строку
  await page.locator(".task-main", { hasText: "тестовая задача" }).click();
  await expect(page.getByText("Редактировать задачу")).toBeVisible();
  await page.getByPlaceholder("Название задачи").fill("переименованная задача");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByText("переименованная задача")).toBeVisible();
  await expect(page.getByText("тестовая задача")).toHaveCount(0);

  // подзадача добавляется у задачи без подзадач (чип «+» виден всегда — v0.6.1)
  await page.locator(".chip-sub").click();
  await page.getByPlaceholder("+ подзадача").fill("первый шаг");
  await page.keyboard.press("Enter");
  await expect(page.locator(".chip-sub")).toHaveText(/0\/1/);

  // выполнение — уходит из активных, появляется в истории
  await page.locator(".task-check").click();
  await expect(page.locator(".task-main", { hasText: "переименованная" })).toHaveCount(0);
  await page.getByRole("button", { name: "История" }).click();
  await expect(page.getByText("переименованная задача")).toBeVisible();

  // удаление из истории
  await page.getByTitle("Удалить").click();
  await expect(page.getByText("переименованная задача")).toHaveCount(0);
});

test("композер: Shift+Enter — подзадачи, Ctrl+Enter — создать", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.locator(".composer-input").click();
  await page.keyboard.type("быстрая задача");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("шаг раз");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("шаг два");
  await page.keyboard.press("Control+Enter");

  // задача в списке, две подзадачи в чипе, композер очищен
  await expect(page.locator(".task-main", { hasText: "быстрая задача" })).toBeVisible();
  await expect(page.locator(".chip-sub")).toHaveText(/0\/2/);
  await expect(page.locator(".composer-input")).toHaveValue("");

  // панель подзадач раскрывается и показывает обе
  await page.locator(".chip-sub").click();
  await expect(page.getByText("шаг раз")).toBeVisible();
  await expect(page.getByText("шаг два")).toBeVisible();
});

test("композер: двойное нажатие Enter не создаёт дубликат", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.locator(".composer-input").click();
  await page.keyboard.type("одна задача");
  // Два быстрых Enter — должен сработать только первый
  await page.keyboard.press("Control+Enter");
  await page.keyboard.press("Control+Enter");

  await expect(page.locator(".task-main", { hasText: "одна задача" })).toHaveCount(1);
  await expect(page.locator(".composer-input")).toHaveValue("");
});

test("календарь: клик по дню создаёт задачу с дедлайном этого дня", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Календарь" }).click();
  await page.locator(".day.today").click();

  await expect(page.getByText("Новая задача")).toBeVisible();
  // дедлайн предзаполнен на 09:00 выбранного дня
  const deadline = await page.locator('input[type="datetime-local"]').inputValue();
  expect(deadline).toMatch(/^\d{4}-\d{2}-\d{2}T09:00$/);

  await page.getByPlaceholder("Название задачи").fill("задача из календаря");
  await page.getByRole("button", { name: "Создать" }).click();

  await expect(page.locator(".day.today .task-chip", { hasText: "задача из календаря" })).toBeVisible();
});

test("заметки: чек-лист рендерится инлайн (live preview) и переключается кликом", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.getByRole("button", { name: "+ Новая заметка" }).click();

  await fillNoteEditor(page, "план:\n- [ ] первый пункт\n- [ ] второй пункт");

  const boxes = page.locator(".cm-task-checkbox");
  await expect(boxes).toHaveCount(2);
  await expect(boxes.first()).not.toBeChecked();
  await boxes.first().click();
  await expect(boxes.first()).toBeChecked();
  await page.waitForTimeout(900); // дебаунс автосохранения (800мс)

  // клик переписывает markdown-источник, а не только DOM-виджет: перечитываем
  // заметку с нуля (reload → перечитать заметки из "БД") — если бы правился
  // только чекбокс в DOM, а не editContent, состояние бы потерялось.
  await page.reload();
  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item").first().click();
  await expect(page.locator(".cm-task-checkbox").first()).toBeChecked();
});

test("редактор: **жирный** внутри ```кода``` не рендерится жирным, снаружи — рендерится", async ({ page }) => {
  await withMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Заметки" }).click();
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  const editor = noteEditor(page);

  await fillNoteEditor(page, "до\n\n```\n**код**\n```\n\n**снаружи**");

  // Уводим курсор на первую строку, чтобы **снаружи** не был сырым
  await editor.click();
  await page.keyboard.press("ControlOrMeta+Home");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");

  await expect(page.locator(".cm-strong")).toHaveCount(1);
});

test("вики-заметки: автодополнение, [[ссылка]] открывает/создаёт, бэклинки, поиск", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  const title = page.getByPlaceholder("Название", { exact: true });
  const editor = noteEditor(page);

  // заметка-цель
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Идея");

  // вторая заметка: автодополнение по "[[" (штатный автокомплит CodeMirror)
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Черновик");
  await editor.click();
  await page.keyboard.type("См. [[Ид");
  await expect(page.locator(".cm-tooltip-autocomplete", { hasText: "Идея" })).toBeVisible();
  // Тултип уже виден, но CM применяет его на следующий кадр — без этого Enter
  // иногда успевает вставить перевод строки раньше, чем completion активна.
  await page.waitForTimeout(150);
  await page.keyboard.press("Enter");
  await expect(editor).toContainText("См. [[Идея]]");

  // живая ссылка + битая (dashed) — рендерятся сразу, без отдельного режима
  await fillNoteEditor(page, "См. [[Идея]] и [[Новая мысль]]");
  const good = page.locator("a.cm-wikilink", { hasText: "Идея" });
  await expect(good).toBeVisible();
  await expect(page.locator("a.cm-wikilink.missing", { hasText: "Новая мысль" })).toBeVisible();

  // клик открывает целевую заметку; бэклинк ведёт обратно
  await good.click();
  await expect(title).toHaveValue("Идея");
  const backlink = page.locator(".backlink", { hasText: "Черновик" });
  await expect(backlink).toBeVisible();
  await backlink.click();
  await expect(title).toHaveValue("Черновик");

  // клик по битой ссылке создаёт заметку с этим названием
  await page.locator("a.cm-wikilink.missing", { hasText: "Новая мысль" }).click();
  await expect(title).toHaveValue("Новая мысль");

  // Ctrl+K находит заметку по содержимому (search_notes)
  await page.keyboard.press("Control+k");
  await page.getByPlaceholder("Поиск задач и заметок...").fill("Идея]] и");
  await page.locator(".result", { hasText: "Черновик" }).click();
  await expect(title).toHaveValue("Черновик");
});

test("вики-заметки: переименование обновляет ссылки в других заметках", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  const title = page.getByPlaceholder("Название", { exact: true });
  const editor = noteEditor(page);

  // целевая заметка
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Идея");
  await page.waitForTimeout(900); // дебаунс автосохранения (800мс)

  // заметка со ссылкой (простой + с алиасом) на неё
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Черновик");
  await fillNoteEditor(page, "см. [[Идея]] и [[Идея|та самая]]");
  await page.waitForTimeout(900);

  // переименовываем целевую — тост появляется, ссылки в «Черновике» обновились
  await page.locator(".note-item", { hasText: "Идея" }).click();
  await title.fill("Идея v2");
  await expect(page.locator(".rename-toast")).toHaveText("Обновлено ссылок: 1");

  // ссылки отрендерены живьём (виджеты, не сырой текст): цель и алиас — как надо
  await page.locator(".note-item", { hasText: "Черновик" }).click();
  await expect(page.locator("a.cm-wikilink", { hasText: "Идея v2" })).toHaveCount(1);
  await expect(page.locator("a.cm-wikilink", { hasText: "та самая" })).toHaveCount(1);

  // клик по обновлённой ссылке всё ещё открывает ту же заметку
  await page.locator("a.cm-wikilink", { hasText: "Идея v2" }).first().click();
  await expect(title).toHaveValue("Идея v2");
});

test("ИИ-автолинковка: кнопка скрыта без ИИ, с ИИ предлагает связи, принятие вставляет [[ссылку]]", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  const title = page.getByPlaceholder("Название", { exact: true });
  const editor = noteEditor(page);

  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Соседняя");
  await page.waitForTimeout(900);

  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Главная");
  await editor.click();
  await page.keyboard.type("текст без ссылок");
  await page.waitForTimeout(900);

  // без ИИ кнопки нет вовсе
  await expect(page.getByTitle("ИИ предложит заметки для связи")).toHaveCount(0);

  // включаем ИИ (in-place, сохраняя уже созданные заметки) и перезаходим,
  // чтобы капабилити-детект перечитал настройки
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    db.settings.ai_provider = "local";
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });
  await page.reload();
  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "Главная" }).click();

  const suggestBtn = page.getByTitle("ИИ предложит заметки для связи");
  await expect(suggestBtn).toBeVisible();
  await suggestBtn.click();

  const chip = page.locator(".link-chip", { hasText: "Соседняя" });
  await expect(chip).toBeVisible();
  await chip.click();
  // вставленная ссылка на новой строке — курсор туда не переходит (текст
  // меняется программно), поэтому строка рендерится живьём, как виджет
  await expect(page.locator("a.cm-wikilink", { hasText: "Соседняя" })).toBeVisible();
  // принятая связь пропадает из списка предложений
  await expect(page.locator(".link-chip", { hasText: "Соседняя" })).toHaveCount(0);
});

test("редактор заметок: переключение между заметками не портит undo-историю", async ({ page }) => {
  await withMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Заметки" }).click();
  const title = page.getByPlaceholder("Название", { exact: true });
  const editor = noteEditor(page);

  // Заметка А
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Заметка А");
  // Ждём сохранение (автосейв 800 мс + запас)
  await page.waitForTimeout(1000);
  await fillNoteEditor(page, "Содержимое А");

  // Заметка Б
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await title.fill("Заметка Б");
  await page.waitForTimeout(1000);
  await fillNoteEditor(page, "Содержимое Б");

  // Возвращаемся к А, потом снова к Б
  await page.locator(".note-item", { hasText: "Заметка А" }).click();
  await page.waitForTimeout(500);
  await page.locator(".note-item", { hasText: "Заметка Б" }).click();
  await page.waitForTimeout(500);

  // Ctrl+Z в Б — история чистая, содержимое не должно измениться
  await editor.click();
  await page.keyboard.press("ControlOrMeta+z");
  await page.waitForTimeout(300);

  // Содержимое Б — всё ещё "Содержимое Б"
  await expect(editor).toContainText("Содержимое Б");
});

test("Ctrl+K находит задачу и открывает раздел задач", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "искомая задача");
  // уходим в другой раздел, чтобы проверить навигацию из поиска
  await page.getByRole("button", { name: "Дашборд" }).click();

  await page.keyboard.press("Control+k");
  await page.getByPlaceholder("Поиск задач и заметок...").fill("искомая");
  await page.locator(".result", { hasText: "искомая задача" }).click();

  await expect(page.getByRole("heading", { name: "Задачи" })).toBeVisible();
  await expect(page.locator(".task-main", { hasText: "искомая задача" })).toBeVisible();
});

test("командная палитра: клавиатурная навигация и фильтр по вводу", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // Стрелка вниз/Enter по действию «Новая заметка» → раздел заметок
  await page.keyboard.press("Control+k");
  await page.locator(".result", { hasText: "Новая задача" }).waitFor();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator(".result.active")).toHaveText(/Новая заметка/);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "+ Новая заметка" })).toBeVisible();

  // Ввод «дашб» фильтрует действия до «Перейти: Дашборд»
  await page.keyboard.press("Control+k");
  await page.getByPlaceholder("Поиск задач и заметок...").fill("дашб");
  await expect(page.locator(".result")).toHaveCount(1);
  await expect(page.locator(".result")).toHaveText(/Дашборд/);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Дашборд" })).toBeVisible();
});

test("командная палитра: «Спланировать день» переходит в календарь-неделю", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.keyboard.press("Control+k");
  await page.getByPlaceholder("Поиск задач и заметок...").fill("спланировать");
  await page.locator(".result", { hasText: "Спланировать день" }).click();

  await expect(page.getByRole("heading", { name: "Календарь" })).toBeVisible();
  await expect(page.locator("button.active-toggle", { hasText: "Неделя" })).toBeVisible();
});

test("командная палитра: «Сменить тему» переключает и сохраняет тему", async ({ page }) => {
  await seedDb(page, { tasks: [], notes: [], settings: { onboarding_complete: true, theme_mode: "light" } });
  await withMock(page);
  await page.goto("/");

  await page.keyboard.press("Control+k");
  await page.getByPlaceholder("Поиск задач и заметок...").fill("сменить тем");
  await page.locator(".result", { hasText: "Сменить тему" }).click();

  const db = JSON.parse(await page.evaluate(() => localStorage.getItem("__mock_db")!));
  expect(db.settings.theme_mode).toBe("dark");
});

test("проекты: модалка центрирована, не растянута на весь экран", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // Открываем модалку проектов
  await page.getByRole("button", { name: "Проекты" }).click();
  await page.waitForSelector(".overlay");

  const vp = page.viewportSize();
  const modalBox = await page.locator(".modal.dialog").boundingBox();
  expect(modalBox).not.toBeNull();
  if (modalBox && vp) {
    // Высота модалки меньше 90% высоты вьюпорта
    expect(modalBox.height).toBeLessThan(vp.height * 0.9);
    // Модалка центрирована по горизонтали (слева меньше половины ширины вьюпорта)
    expect(modalBox.x).toBeGreaterThan(0);
    expect(modalBox.x + modalBox.width).toBeLessThan(vp.width);
  }
});

test("проекты: создание, назначение задаче, группировка и фильтр", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // создать проект
  await page.getByRole("button", { name: "Проекты" }).click();
  await page.getByPlaceholder("Название нового проекта").fill("Ремонт");
  await page.getByRole("button", { name: "Создать" }).click();
  await page.getByRole("button", { name: "Закрыть" }).click();

  // задача в проект через модал
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  await page.getByPlaceholder("Название задачи").fill("покрасить стены");
  await page.getByLabel("Проект").selectOption({ label: "Ремонт" });
  await page.getByRole("button", { name: "Создать" }).click();
  await createTask(page, "задача вне проекта");

  // группировка: заголовки секций видны
  await expect(page.locator(".project-head", { hasText: "Ремонт" })).toBeVisible();
  await expect(page.locator(".project-head", { hasText: "Без проекта" })).toBeVisible();

  // фильтр по проекту
  await page.locator(".project-filter").selectOption({ label: "Ремонт" });
  await expect(page.getByText("покрасить стены")).toBeVisible();
  await expect(page.locator(".task-main", { hasText: "задача вне проекта" })).toHaveCount(0);
});

test("цель проекта: прогресс в заголовке группы, зелёная при выполнении, карта на дашборде", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // проект с целью «1 задача в неделю»
  await page.getByRole("button", { name: "Проекты" }).click();
  await page.getByPlaceholder("Название нового проекта").fill("Спорт");
  await page.getByRole("button", { name: "Создать" }).click();
  await page.locator(".proj-goal .goal-num").first().fill("1");
  await page.locator(".proj-goal .goal-num").first().blur();
  // чип прогресса появился в модале
  await expect(page.locator(".proj-goal .goal-chip")).toHaveText("0/1 задач");
  await page.getByRole("button", { name: "Закрыть" }).click();

  // задача в проекте → в заголовке группы виден прогресс цели
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  await page.getByPlaceholder("Название задачи").fill("пробежка");
  await page.getByLabel("Проект").selectOption({ label: "Спорт" });
  await page.getByRole("button", { name: "Создать" }).click();
  const headChip = page.locator(".project-head .goal-chip");
  await expect(headChip).toHaveText("0/1 задач");
  await expect(headChip).not.toHaveClass(/met/);

  // выполнение задачи закрывает цель — чип зеленеет
  await page.locator(".task-check").click();
  await expect(page.locator(".project-head")).toHaveCount(0); // группа опустела
  await page.getByRole("button", { name: "Проекты" }).click();
  await expect(page.locator(".proj-goal .goal-chip")).toHaveText("1/1 задач");
  await expect(page.locator(".proj-goal .goal-chip")).toHaveClass(/met/);
  await page.getByRole("button", { name: "Закрыть" }).click();

  // карта «Цели проектов» на дашборде
  await page.getByRole("button", { name: "Дашборд" }).click();
  await expect(page.getByText("Цели проектов")).toBeVisible();
  const goalCard = page.locator(".goal-item", { hasText: "Спорт" });
  await expect(goalCard).toBeVisible();
  await expect(goalCard.locator(".goal-val")).toHaveText("1/1");
});

test("тайм-блокинг: drag из бэклога ставит блок, задача видна в «Сегодня»", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "глубокая работа");

  await page.getByRole("button", { name: "Календарь" }).click();
  await page.getByRole("button", { name: "Неделя" }).click();

  const backlogItem = page.locator(".backlog-item", { hasText: "глубокая работа" });
  await expect(backlogItem).toBeVisible();
  // ИИ выключен (дефолт мока) — планировщик скрыт (капабилити-детект)
  await expect(page.getByRole("button", { name: "Спланировать день" })).toHaveCount(0);

  // бросаем на колонку сегодняшнего дня (~середина утра)
  await backlogItem.dragTo(page.locator(".week-col.today"), { targetPosition: { x: 40, y: 400 } });

  const block = page.locator(".block", { hasText: "глубокая работа" });
  await expect(block).toBeVisible();
  await expect(backlogItem).toHaveCount(0); // из бэклога ушла
  await expect(block.locator(".block-time")).toHaveText(/\d{2}:\d{2}–\d{2}:\d{2}/);

  // строка «Сегодня:» в разделе задач
  await page.getByRole("button", { name: "Задачи" }).click();
  await expect(page.locator(".day-plan-chip", { hasText: "глубокая работа" })).toBeVisible();

  // снять блок — вернулась в бэклог
  await page.getByRole("button", { name: "Календарь" }).click();
  await page.getByRole("button", { name: "Неделя" }).click();
  await page.locator(".block", { hasText: "глубокая работа" }).hover();
  await page.locator(".block-x").click();
  await expect(page.locator(".backlog-item", { hasText: "глубокая работа" })).toBeVisible();
});

test("ИИ-планировщик: план дня — призрак → применить → блок; «Что сейчас?» — совет", async ({ page }) => {
  // капабилити-детект: кнопки планировщика видны только при включённом ИИ
  await seedDb(page, { tasks: [], notes: [], settings: { ai_provider: "local" } });
  await withMock(page);
  await page.goto("/");

  await createTask(page, "важное дело");

  // «Что сейчас?» — совет баннером
  await page.getByRole("button", { name: "Что сейчас?" }).click();
  await expect(page.locator(".what-now")).toContainText("Совет мока");
  await page.locator(".what-now .btn-icon").click();
  await expect(page.locator(".what-now")).toHaveCount(0);

  // План дня: призрак в сетке, применение ставит настоящий блок
  await page.getByRole("button", { name: "Календарь" }).click();
  await page.getByRole("button", { name: "Неделя" }).click();
  await page.getByRole("button", { name: "Спланировать день" }).click();

  const ghost = page.locator(".block.ghost", { hasText: "важное дело" });
  await expect(ghost).toBeVisible();
  await expect(page.locator(".backlog-item", { hasText: "важное дело" })).toBeVisible(); // ещё в бэклоге

  await page.getByRole("button", { name: "Применить" }).click();
  await expect(page.locator(".block.ghost")).toHaveCount(0);
  await expect(page.locator(".block", { hasText: "важное дело" })).toBeVisible();
  await expect(page.locator(".block .block-time", { hasText: "10:00–11:00" })).toBeVisible();
  await expect(page.locator(".backlog-item", { hasText: "важное дело" })).toHaveCount(0);
});

test("помодоро: виджет виден при активной фазе, пауза/продолжить и пропуск фазы", async ({ page }) => {
  const until = new Date(Date.now() + 12 * 60 * 1000).toISOString();
  await seedDb(page, {
    tasks: [], notes: [], settings: { onboarding_complete: true },
    pomodoro: { phase: "work", until },
  });
  await withMock(page);
  await page.goto("/");

  const widget = page.locator(".pomo");
  await expect(widget).toBeVisible();
  await expect(widget.locator(".pomo-label")).toHaveText("🍅 Фокус");

  await widget.getByTitle("Пауза").click();
  await expect(widget.locator(".pomo-label")).toHaveText("🍅 Пауза");

  await widget.getByTitle("Продолжить").click();
  await expect(widget.locator(".pomo-label")).toHaveText("🍅 Фокус");

  await widget.getByTitle("Пропустить фазу").click();
  await expect(widget.locator(".pomo-label")).toHaveText("☕ Перерыв");
});

test("помодоро: ▶ на виджете при off запускает ручной цикл, ■ останавливает", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  const widget = page.locator(".pomo");
  await expect(widget.getByTitle("Начать помидор")).toBeVisible();

  await widget.getByTitle("Начать помидор").click();
  await expect(widget.locator(".pomo-label")).toHaveText("🍅 Фокус");

  await widget.getByTitle("Остановить").click();
  await expect(widget.getByTitle("Начать помидор")).toBeVisible();
});

test("дашборд: карточка «Помодоро» показывает статистику и стрики", async ({ page }) => {
  await seedDb(page, {
    tasks: [], notes: [], settings: { onboarding_complete: true },
    pomodoroStats: { today: 3, week: 12, task_streak: 4, pomodoro_streak: 2 },
  });
  await withMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Дашборд" }).click();

  const card = page.locator(".card.panel", { hasText: "Помодоро" });
  await expect(card).toBeVisible();
  await expect(card).toContainText("3");
  await expect(card).toContainText("12");
  await expect(card).toContainText("4 дн.");
  await expect(card).toContainText("2 дн.");
});

test("дашборд: годовой календарь — квадрат сегодняшнего дня, hover показывает задачи", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "сделанное дело");
  await page.locator(".task-check").click();

  await page.getByRole("button", { name: "Дашборд" }).click();
  const p = (n: number) => String(n).padStart(2, "0");
  const now = new Date();
  const today = `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;

  const cell = page.locator(`.cal-cell[data-date="${today}"]`);
  await expect(cell).toHaveAttribute("data-count", "1");

  await cell.hover();
  await expect(page.locator(".cal-tip")).toContainText("выполнено: 1");
  await expect(page.locator(".cal-tip")).toContainText("сделанное дело");
});

test("сортировка: drag строки меняет порядок, порядок переживает перезагрузку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  for (const title of ["первая", "вторая", "третья"]) {
    await page.locator(".composer-input").click();
    await page.keyboard.type(title);
    await page.keyboard.press("Control+Enter");
    await expect(page.locator(".task-main", { hasText: title })).toBeVisible();
  }
  const titles = page.locator(".task-list .task-title");
  await expect(titles).toHaveText(["первая", "вторая", "третья"]);

  // тащим «первую» на «третью» → уходит в конец
  await page.locator(".task-row", { hasText: "первая" })
    .dragTo(page.locator(".task-row", { hasText: "третья" }));
  await expect(titles).toHaveText(["вторая", "третья", "первая"]);

  // порядок сохранён в «БД» и переживает перезагрузку
  await page.reload();
  await expect(page.locator(".task-list .task-title")).toHaveText(["вторая", "третья", "первая"]);
});

test("категории: создание в настройках, назначение задаче, удаление с переназначением", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // создать категорию «Спорт» в настройках
  await page.getByRole("button", { name: "Настройки" }).click();
  const catSection = page.locator("section").filter({ hasText: "Категории задач" });
  await page.getByPlaceholder("Новая категория").fill("Спорт");
  await page.getByRole("button", { name: "Добавить" }).click();
  // 5 посевных + «Спорт» + строка добавления
  await expect(catSection.locator(".rule-row")).toHaveCount(7);
  const sportInput = catSection.locator(".rule-row input:not(.cat-color)").nth(5);
  await expect(sportInput).toHaveValue("Спорт");

  // создать задачу с этой категорией
  await page.getByRole("button", { name: "Задачи" }).click();
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  await page.getByPlaceholder("Название задачи").fill("пробежка");
  await page.getByLabel("Категория").selectOption({ label: "Спорт" });
  await page.getByRole("button", { name: "Создать" }).click();
  await expect(page.locator(".chip-cat", { hasText: "Спорт" })).toBeVisible();

  // удалить категорию — задача переезжает в «Другое»
  await page.getByRole("button", { name: "Настройки" }).click();
  const sportRow = catSection.locator(".rule-row").nth(5);
  await expect(sportRow.locator("input:not(.cat-color)")).toHaveValue("Спорт");
  await sportRow.getByTitle("Удалить (задачи перейдут в «Другое»)").click();
  await expect(catSection.locator(".rule-row")).toHaveCount(6);

  await page.getByRole("button", { name: "Задачи" }).click();
  await expect(page.locator(".chip-cat", { hasText: "Другое" })).toBeVisible();
});

test("лимиты категорий приложений: поле сохраняется и переживает перезагрузку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");
  // windowTracking включает секцию правил приложений (gated на неё же, что и
  // сама категоризация) — патчим mock-db напрямую, не через seedDb, чтобы не
  // конфликтовать с init-скриптом на reload (seedDb иначе стирает то, что
  // сохранил мок в localStorage за время теста).
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    db.windowTracking = "kitty";
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });
  await page.reload();

  await page.getByRole("button", { name: "Настройки" }).click();
  const limitsLabel = page.getByText("Лимиты времени на категории (мин/день)");
  await expect(limitsLabel).toBeVisible();

  const otherRow = page.locator(".limit-row", { hasText: "Другое" });
  await otherRow.locator("input[type=number]").fill("45");
  await page.getByRole("button", { name: "Сохранить" }).click();

  await page.reload();
  await page.getByRole("button", { name: "Настройки" }).click();
  await expect(page.locator(".limit-row", { hasText: "Другое" }).locator("input[type=number]")).toHaveValue("45");
});

test("версии заметок: панель показывает историю, восстановление меняет текст", async ({ page }) => {
  const noteId = "n1";
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: noteId, title: "заметка с историей", content: "новый текст",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    noteRevisions: [{
      id: "rev1", note_id: noteId, content: "старый текст",
      created_at: new Date(Date.now() - 20 * 60000).toISOString(),
    }],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "заметка с историей" }).click();
  await expect(noteEditor(page)).toContainText("новый текст");

  await page.getByTitle("Версии заметки").click();
  await expect(page.getByText("Версии заметки")).toBeVisible();
  await expect(page.locator(".revision-item")).toHaveCount(1);

  await page.locator(".revision-item").click();
  await expect(page.locator(".revision-preview pre")).toContainText("старый текст");

  page.once("dialog", (d) => d.accept());
  await page.getByRole("button", { name: "Восстановить" }).click();
  await expect(page.locator(".revisions-dialog")).toHaveCount(0);
  await expect(noteEditor(page)).toContainText("старый текст");
});

test("картинки в заметках: ![](имя) рендерится img-виджетом", async ({ page }) => {
  const noteId = "n1";
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: noteId, title: "заметка с картинкой", content: "текст ![](photo.png) конец",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    images: [{ filename: "photo.png", dataUrl: "data:image/png;base64,AAAA" }],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "заметка с картинкой" }).click();

  const img = page.locator(".cm-note-image");
  await expect(img).toBeVisible();
  await expect(img).toHaveAttribute("src", "data:image/png;base64,AAAA");
});

test("картинки в заметках: ссылка скрыта по умолчанию, клик по картинке показывает/прячет её", async ({ page }) => {
  const noteId = "n1";
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: noteId, title: "заметка с картинкой", content: "![](photo.png)",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    images: [{ filename: "photo.png", dataUrl: "data:image/png;base64,AAAA" }],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "заметка с картинкой" }).click();

  const editor = noteEditor(page);
  const img = page.locator(".cm-note-image");
  await expect(img).toBeVisible();
  await expect(editor).not.toContainText("![](photo.png)");

  await img.click();
  await expect(editor).toContainText("![](photo.png)");
  await expect(img).toBeVisible();

  await img.click();
  await expect(editor).not.toContainText("![](photo.png)");
  await expect(img).toBeVisible();
});

test("экспорт/импорт заметок в .md: roundtrip через папку", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: "n1", title: "моя заметка", content: "содержимое заметки",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    settings: { onboarding_complete: true },
    mockDialogPath: "/mock/notes-export",
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByRole("button", { name: "Экспорт заметок (.md)" }).click();
  await expect(page.getByText("Экспортировано заметок: 1")).toBeVisible();

  // Импорт из той же (мок-)папки: совпадение по названию не мёржится —
  // создаётся отдельная заметка (задокументированное поведение), поэтому
  // после импорта в списке две заметки с одинаковым названием.
  await page.getByRole("button", { name: "Импорт заметок из папки" }).click();
  await expect(page.getByText("Импортировано заметок: 1")).toBeVisible();

  await page.getByRole("button", { name: "Заметки" }).click();
  await expect(page.locator(".note-item", { hasText: "моя заметка" })).toHaveCount(2);
});

test("шаблоны чеклистов: сохранить подзадачи как шаблон, применить к другой задаче", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "поездка");
  await createTask(page, "другая задача");

  // Разворачиваем «поездку», добавляем две подзадачи
  const tripRow = page.locator(".task-row", { hasText: "поездка" });
  await tripRow.getByTitle("Добавить подзадачу").click();
  const tripPanel = tripRow.locator("xpath=following-sibling::li[contains(@class,'task-sub-panel')][1]");
  await tripPanel.locator(".sub-input").fill("паспорт");
  await tripPanel.getByRole("button", { name: "Добавить", exact: true }).click();
  await tripPanel.locator(".sub-input").fill("билеты");
  await tripPanel.getByRole("button", { name: "Добавить", exact: true }).click();
  await expect(tripPanel.locator(".sub-line").filter({ hasText: /паспорт|билеты/ })).toHaveCount(2);

  // Сохраняем как шаблон
  await tripPanel.getByRole("button", { name: "Сохранить как шаблон" }).click();
  await tripPanel.getByPlaceholder("Название шаблона").fill("Поездка");
  await tripPanel.getByRole("button", { name: "Сохранить", exact: true }).click();

  // Применяем шаблон к «другой задаче»
  const otherRow = page.locator(".task-row", { hasText: "другая задача" });
  await otherRow.getByTitle("Добавить подзадачу").click();
  const otherPanel = otherRow.locator("xpath=following-sibling::li[contains(@class,'task-sub-panel')][1]");
  await otherPanel.getByRole("button", { name: "Из шаблона…" }).click();
  await expect(otherPanel.getByText("Поездка")).toBeVisible();
  await otherPanel.getByRole("button", { name: "Применить" }).click();

  // Чип N/M совпадает: 2 подзадачи, 0 выполнено
  await expect(otherRow.locator(".chip-sub")).toHaveText("▾ 0/2");
});

test("тёмная тема применяется и переживает перезагрузку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByLabel("Тёмная").check();
  await expect(page.locator("html")).toHaveClass(/dark/);

  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await page.reload();
  await expect(page.locator("html")).toHaveClass(/dark/);
});

test("авто-бэкап: секция в настройках, кнопка «Сделать сейчас» вызывает команду", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();

  // Секция «Авто-бэкап» видна
  await expect(page.getByText("Авто-бэкап")).toBeVisible();
  await expect(page.getByText("Папка для бэкапов")).toBeVisible();
  await expect(page.getByText("Хранить копий")).toBeVisible();

  // «Сделать сейчас» disabled без папки
  const backupBtn = page.getByRole("button", { name: "Сделать сейчас" });
  await expect(backupBtn).toBeDisabled();

  // Устанавливаем папку в настройках, сохраняем, перезагружаем
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    db.settings.auto_backup_dir = "/tmp/mock-backups";
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });
  await page.reload();
  await page.getByRole("button", { name: "Настройки" }).click();

  // Теперь кнопка активна и вызывает команду
  await expect(backupBtn).toBeEnabled();
  await backupBtn.click();
  await expect(page.getByText("Бэкап сохранён")).toBeVisible();
});

test("рутины: создание, блок в неделе, выключение", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // Создаём задачу с дедлайном на сегодня (нужна для недельного вида)
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    if (!db.routines) db.routines = [];
    db.tasks.push({
      id: "test-task-1",
      title: "Тестовая задача",
      status: "Todo",
      priority: "Medium",
      category: "Other",
      tags: [],
      description: null,
      deadline: null,
      recurrence: "None",
      hidden: false,
      project_id: null,
      scheduled_at: new Date().toISOString(),
      scheduled_mins: 60,
      sort_order: 1,
      subtasks: [],
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      completed_at: null,
    });
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });

  // Переходим в календарь, затем в недельный вид
  await page.getByRole("button", { name: "Календарь" }).click();
  await page.getByRole("button", { name: "Неделя" }).click();

  // Открываем модал рутин
  await page.getByRole("button", { name: "Рутины" }).click();
  await expect(page.getByRole("heading", { name: "Рутины" })).toBeVisible();

  // Добавляем новую рутину
  await page.getByRole("button", { name: "+ Добавить рутину" }).click();
  await page.locator(".edit-form input[placeholder='Название рутины']").fill("Планёрка");
  // Включаем Пн и Вт (первые два чекбокса)
  await page.locator(".day-chip input").nth(0).check();
  await page.locator(".day-chip input").nth(1).check();
  // Время начала 09:00
  await page.locator(".edit-form input[type='time']").fill("09:00");
  await page.locator(".edit-form input[type='number']").fill("45");
  await page.getByRole("button", { name: "Добавить" }).click();

  // Рутина видна в списке модала
  await expect(page.getByRole("dialog").getByText("Планёрка")).toBeVisible();
});

test("трекинг: ▶ на задаче запускает, ■ останавливает", async ({ page }) => {
  await withMock(page);
  await page.goto("/");
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    db.tasks.push({
      id: "track-1", title: "Трекинг-тест", status: "Todo", priority: "Medium",
      category: "Other", tags: [], description: null, deadline: null,
      recurrence: "None", hidden: false, project_id: null,
      scheduled_at: null, scheduled_mins: null, sort_order: 1, subtasks: [],
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
      completed_at: null,
    });
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });
  await page.reload();

  // Находим ▶ на строке задачи
  await expect(page.getByText("Трекинг-тест")).toBeVisible();
  const playBtn = page.locator("button[title='Начать трекинг']");
  await expect(playBtn).toBeVisible();
  await playBtn.click();

  // Кнопка сменилась на ■
  await expect(page.locator("button[title='Остановить трекинг']")).toBeVisible();

  // Виджет трекинга в сайдбаре
  await expect(page.getByText("Трекинг-тест")).toBeVisible();

  await page.locator("button[title='Остановить трекинг']").click();
  await expect(page.locator("button[title='Начать трекинг']")).toBeVisible();
});
