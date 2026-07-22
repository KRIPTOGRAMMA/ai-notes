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
  // exact — иначе матчится и «Сохранить как шаблон» (шаблоны чеклистов, v0.8.3)
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(page.getByText("переименованная задача")).toBeVisible();
  await expect(page.getByText("тестовая задача")).toHaveCount(0);

  // подзадача добавляется у задачи без подзадач (чип «+» виден всегда — v0.6.1)
  await page.locator(".chip-sub").click();
  await page.getByPlaceholder("+ подзадача").fill("первый шаг");
  await page.keyboard.press("Enter");
  await expect(page.locator(".chip-sub")).toHaveText(/0\/1/);

  // v0.8.2: чип с подзадачами визуально выделен; все выполнены — зеленеет
  await expect(page.locator(".chip-sub")).toHaveClass(/has-subs/);
  await page.locator(".task-sub-panel input[type='checkbox']").check();
  await expect(page.locator(".chip-sub")).toHaveClass(/subs-done/);
  await expect(page.locator(".chip-sub")).toHaveText(/1\/1/);

  // выполнение — уходит из активных, появляется в истории
  await page.locator(".task-check").click();
  await expect(page.locator(".task-main", { hasText: "переименованная" })).toHaveCount(0);
  await page.getByRole("button", { name: "История" }).click();
  await expect(page.getByText("переименованная задача")).toBeVisible();

  // удаление из истории — мягкое (v0.8.12): уходит из истории, но не исчезает насовсем
  await page.getByTitle("Удалить").click();
  await expect(page.getByText("переименованная задача")).toHaveCount(0);

  await page.getByRole("button", { name: "Корзина" }).click();
  await expect(page.locator(".task-list.history", { hasText: "переименованная задача" })).toBeVisible();
});

test("повтор по дням недели: выбор в модалке сохраняется и отображается индикатором", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  const modal = page.locator(".modal");
  await modal.getByPlaceholder("Название задачи").fill("зарядка");
  await modal.getByLabel("Повтор").selectOption("Weekdays");

  const dayPicker = modal.locator(".day-picker");
  await expect(dayPicker).toBeVisible();
  await dayPicker.locator(".day-chip", { hasText: "Пн" }).locator("input").check();
  await dayPicker.locator(".day-chip", { hasText: "Ср" }).locator("input").check();
  await dayPicker.locator(".day-chip", { hasText: "Пт" }).locator("input").check();

  await modal.getByRole("button", { name: "Создать" }).click();

  const row = page.locator(".task-row", { hasText: "зарядка" });
  await expect(row.locator(".muted[title*='Пн']")).toHaveCount(1);

  // Редактирование — чекбоксы восстанавливаются из сохранённой маски
  await row.locator(".task-main").click();
  const editModal = page.locator(".modal");
  await expect(editModal.getByLabel("Повтор")).toHaveValue("Weekdays");
  const editPicker = editModal.locator(".day-picker");
  await expect(editPicker.locator(".day-chip", { hasText: "Пн" }).locator("input")).toBeChecked();
  await expect(editPicker.locator(".day-chip", { hasText: "Ср" }).locator("input")).toBeChecked();
  await expect(editPicker.locator(".day-chip", { hasText: "Пт" }).locator("input")).toBeChecked();
  await expect(editPicker.locator(".day-chip", { hasText: "Вт" }).locator("input")).not.toBeChecked();
  await editModal.getByRole("button", { name: "Отмена" }).click();
});

test("корзина: мягкое удаление, восстановление возвращает в активные, «навсегда» удаляет", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // Скоуп на панель корзины (после заголовка «Корзина») — активные задачи
  // тоже рендерятся как .task-row, простой .task-row-локатор был бы неоднозначен.
  const trashPanel = page.locator(".section-title", { hasText: "Корзина" }).locator("xpath=following-sibling::*[1]");

  await createTask(page, "черновик задачи");
  await expect(page.locator(".task-main", { hasText: "черновик задачи" })).toBeVisible();

  await page.locator(".task-row", { hasText: "черновик задачи" }).getByTitle("Удалить").click();
  await expect(page.locator(".task-main", { hasText: "черновик задачи" })).toHaveCount(0);

  // В корзине, не в истории
  await page.getByRole("button", { name: "История" }).click();
  await expect(page.getByText("черновик задачи")).toHaveCount(0);
  await page.getByRole("button", { name: "История" }).click(); // закрыть

  await page.getByRole("button", { name: "Корзина" }).click();
  const trashRow = trashPanel.locator(".task-row", { hasText: "черновик задачи" });
  await expect(trashRow).toBeVisible();

  // Восстановить — снова в активных, из корзины пропадает
  await trashRow.getByRole("button", { name: "Восстановить" }).click();
  await expect(trashPanel.locator(".task-row", { hasText: "черновик задачи" })).toHaveCount(0);
  await expect(page.locator(".task-main", { hasText: "черновик задачи" })).toBeVisible();

  // Удалить снова, затем стереть навсегда
  await page.locator(".task-row", { hasText: "черновик задачи" }).getByTitle("Удалить").click();
  await expect(trashPanel.locator(".task-row", { hasText: "черновик задачи" })).toBeVisible();
  await trashPanel.locator(".task-row", { hasText: "черновик задачи" }).getByTitle("Удалить навсегда").click();
  await expect(trashPanel.locator(".task-row", { hasText: "черновик задачи" })).toHaveCount(0);

  await page.reload();
  await page.getByRole("button", { name: "Корзина" }).click();
  await expect(page.getByText("Корзина пуста")).toBeVisible();
});

test("история: клик по строке открывает read-only детали с подзадачами и датой завершения", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "поход в горы");
  await page.locator(".chip-sub").click();
  const draft = page.getByPlaceholder("+ подзадача (Enter)");
  await draft.fill("рюкзак");
  await draft.press("Enter");
  await page.locator(".check-input").last().fill("палатка");
  await page.locator(".check-input").last().press("Enter");
  await page.locator(".task-sub-panel input[type='checkbox']").first().check();

  await page.locator(".task-check").click();
  await page.getByRole("button", { name: "История" }).click();

  await page.locator(".history .task-main", { hasText: "поход в горы" }).click();
  const modal = page.locator(".modal");
  await expect(modal.getByText("поход в горы")).toBeVisible();
  await expect(modal.locator(".check-row")).toHaveCount(2);
  await expect(modal.locator(".check-row").nth(0).locator("input")).toBeChecked();
  await expect(modal.locator(".check-row").nth(1).locator("input")).not.toBeChecked();
  await expect(modal.getByText("Завершена")).toBeVisible();

  await modal.getByRole("button", { name: "Закрыть" }).click();
  await expect(page.locator(".modal")).toHaveCount(0);
});

test("модалка: инлайн-чеклист подзадач — Enter добавляет строку, сохранение применяет diff", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  // создание: две строки чеклиста через Enter (скоуп .modal — в панели строки
  // задач те же .check-input, глобальный локатор был бы неоднозначным)
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  await page.getByPlaceholder("Название задачи").fill("поездка");
  const inputs = page.locator(".modal .check-input");
  await inputs.nth(0).fill("паспорт");
  await inputs.nth(0).press("Enter");
  await inputs.nth(1).fill("билеты");
  await page.getByRole("button", { name: "Создать" }).click();
  await expect(page.locator(".chip-sub")).toHaveText(/0\/2/);

  // редактирование: rename + удаление строки Backspace-ом на пустой
  await page.locator(".task-main", { hasText: "поездка" }).click();
  await expect(inputs.nth(0)).toHaveValue("паспорт");
  await expect(inputs.nth(1)).toHaveValue("билеты");
  await inputs.nth(0).fill("загранпаспорт");
  await inputs.nth(1).fill("");
  await inputs.nth(1).press("Backspace");
  // exact — иначе матчится и «Сохранить как шаблон» в авто-развёрнутой панели
  await page.locator(".modal").getByRole("button", { name: "Сохранить", exact: true }).click();

  await expect(page.locator(".chip-sub")).toHaveText(/0\/1/);
  // v0.8.3: панель авто-развёрнута (show_subtasks_expanded по умолчанию)
  await expect(page.locator(".task-sub-panel .check-input").nth(0)).toHaveValue("загранпаспорт");
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

  // v0.8.3: панель авто-развёрнута, подзадачи — инлайн-инпуты чеклиста
  const panelInputs = page.locator(".task-sub-panel .check-input");
  await expect(panelInputs.nth(0)).toHaveValue("шаг раз");
  await expect(panelInputs.nth(1)).toHaveValue("шаг два");
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
  await expect(widget.locator(".pomo-label")).toHaveText("Фокус");

  await widget.getByTitle("Пауза").click();
  await expect(widget.locator(".pomo-label")).toHaveText("Пауза");

  await widget.getByTitle("Продолжить").click();
  await expect(widget.locator(".pomo-label")).toHaveText("Фокус");

  await widget.getByTitle("Пропустить фазу").click();
  await expect(widget.locator(".pomo-label")).toHaveText("Перерыв");
});

test("помодоро: ▶ на виджете при off запускает ручной цикл, ■ останавливает", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  const widget = page.locator(".pomo");
  await expect(widget.getByTitle("Начать помидор")).toBeVisible();

  await widget.getByTitle("Начать помидор").click();
  await expect(widget.locator(".pomo-label")).toHaveText("Фокус");

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

test("дашборд: клик по дню открывает попап, клик по задаче ведёт в раздел Задач", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await createTask(page, "сделанное дело");
  await page.locator(".task-check").click();

  await page.getByRole("button", { name: "Дашборд" }).click();
  const p = (n: number) => String(n).padStart(2, "0");
  const now = new Date();
  const today = `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;

  const cell = page.locator(`.cal-cell[data-date="${today}"]`);
  await cell.click();

  const popup = page.locator(".cal-popup");
  await expect(popup).toBeVisible();
  await expect(popup).toContainText("выполнено: 1");
  await popup.getByRole("button", { name: "сделанное дело" }).click();

  await expect(page.locator(".cal-popup")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Задачи" })).toBeVisible();

  // Регресс: задача завершена (история) — должна открыться read-only
  // TaskHistoryDetail, а не редактируемая TaskModal (без select "Повтор"/
  // "Редактировать задачу" в заголовке — тех полей у выполненной задачи
  // уже нет смысла трогать).
  await expect(page.locator(".dialog-title", { hasText: "сделанное дело" })).toBeVisible();
  await expect(page.getByLabel("Повтор")).toHaveCount(0);
  await expect(page.getByText("Редактировать задачу")).toHaveCount(0);
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

  // создать категорию «Спорт» в настройках (вкладка «Категории»)
  await page.getByRole("button", { name: "Настройки" }).click();
  await page.locator(".settings-tab", { hasText: "Категории" }).click();
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
  await page.locator(".settings-tab", { hasText: "Категории" }).click();
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
  await page.locator(".settings-tab", { hasText: "Категории" }).click();
  const limitsLabel = page.getByText("Лимиты времени на категории (мин/день)");
  await expect(limitsLabel).toBeVisible();

  const otherRow = page.locator(".limit-row", { hasText: "Другое" });
  await otherRow.locator("input[type=number]").fill("45");
  await page.getByRole("button", { name: "Сохранить" }).click();

  await page.reload();
  await page.getByRole("button", { name: "Настройки" }).click();
  await page.locator(".settings-tab", { hasText: "Категории" }).click();
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

test("граф заметок: связанные заметки дают узлы и ребро, изолированная — узел без связей", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [
      {
        id: "n1", title: "Идея A", content: "см. [[Заметка Б]]",
        tags: [], linked_task_id: null, project_id: null,
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
      },
      {
        id: "n2", title: "Заметка Б", content: "ссылается назад на [[Идея A]]",
        tags: [], linked_task_id: null, project_id: null,
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
      },
      {
        id: "n3", title: "Одинокая заметка", content: "без ссылок",
        tags: [], linked_task_id: null, project_id: null,
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
      },
    ],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Граф" }).click();

  await expect(page.getByText("3 заметок · 1 связей")).toBeVisible();
  const nodes = page.locator(".node");
  await expect(nodes).toHaveCount(3);
  await expect(page.locator(".node.isolated")).toHaveCount(1);
  await expect(page.locator(".edge")).toHaveCount(1);

  // Двойной клик по узлу открывает заметку в разделе «Заметки»
  await page.locator(".node", { hasText: "Идея A" }).dblclick();
  await expect(page.locator(".note-item.active", { hasText: "Идея A" })).toBeVisible();
});

test("закрепление заметок: пин поднимает заметку наверх списка, переживает перезагрузку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");
  // Сеем заметки напрямую в localStorage мока (не через seedDb-init-script):
  // seedDb регистрирует свой initScript, который заново стирает localStorage
  // на каждый page.reload() — теряя то, что сохранил мок за время теста
  // (тот же грабли, что и в тесте «лимиты категорий приложений» выше).
  await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db")!);
    db.notes = [
      { id: "n1", title: "Первая заметка", content: "текст 1", tags: [], linked_task_id: null, project_id: null, pinned: false, created_at: new Date(Date.now() - 2000).toISOString(), updated_at: new Date(Date.now() - 2000).toISOString() },
      { id: "n2", title: "Вторая заметка", content: "текст 2", tags: [], linked_task_id: null, project_id: null, pinned: false, created_at: new Date(Date.now() - 1000).toISOString(), updated_at: new Date(Date.now() - 1000).toISOString() },
      { id: "n3", title: "Третья заметка", content: "текст 3", tags: [], linked_task_id: null, project_id: null, pinned: false, created_at: new Date().toISOString(), updated_at: new Date().toISOString() },
    ];
    localStorage.setItem("__mock_db", JSON.stringify(db));
  });
  await page.reload();
  await page.getByRole("button", { name: "Заметки" }).click();

  const rows = page.locator(".note-row");
  await expect(rows).toHaveCount(3);
  const unpinnedFirstTitle = await rows.nth(0).locator(".note-title").innerText();

  // Закрепляем самую старую — "Первая заметка" — она должна подняться наверх
  const firstRow = page.locator(".note-row", { hasText: "Первая заметка" });
  await firstRow.locator(".pin-btn").click({ force: true });
  await expect(rows.nth(0)).toContainText("Первая заметка");
  await expect(page.locator(".pin-btn.pinned")).toHaveCount(1);

  // Переживает перезагрузку
  await page.reload();
  await page.getByRole("button", { name: "Заметки" }).click();
  await expect(page.locator(".note-row").nth(0)).toContainText("Первая заметка");
  await expect(page.locator(".pin-btn.pinned")).toHaveCount(1);

  // Открепление возвращает к порядку без пина (тот же, что был до закрепления)
  await page.locator(".note-row", { hasText: "Первая заметка" }).locator(".pin-btn").click({ force: true });
  await expect(page.locator(".note-row").nth(0)).toContainText(unpinnedFirstTitle);
  await expect(page.locator(".pin-btn.pinned")).toHaveCount(0);
});

test("zen-режим редактора: кнопка и хоткей раскрывают на весь экран, скрывают панель и мету, Esc закрывает", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: "n1", title: "заметка для дзена", content: "текст заметки",
      tags: ["важное"], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "заметка для дзена" }).click();

  await expect(page.locator(".list-pane")).toBeVisible();
  await expect(page.locator(".editor-meta")).toBeVisible();

  await page.getByTitle("Zen-режим (Ctrl+Shift+Z)").click();
  await expect(page.locator(".editor-pane.zen")).toBeVisible();
  await expect(page.locator(".editor-meta")).toHaveCount(0);
  await expect(noteEditor(page)).toBeVisible();
  // Список заметок технически остаётся в DOM (то же .notes-дерево), но
  // редактор — fixed-оверлей поверх него на весь экран.
  await expect(page.locator(".editor-pane.zen")).toHaveCSS("position", "fixed");

  await page.keyboard.press("Escape");
  await expect(page.locator(".editor-pane.zen")).toHaveCount(0);
  await expect(page.locator(".editor-meta")).toBeVisible();

  // Хоткей включает и выключает так же, как кнопка
  await page.keyboard.press("Control+Shift+KeyZ");
  await expect(page.locator(".editor-pane.zen")).toBeVisible();
  await page.keyboard.press("Control+Shift+KeyZ");
  await expect(page.locator(".editor-pane.zen")).toHaveCount(0);
});

test("панель форматирования: кнопки оборачивают выделение markdown-маркерами, Ctrl+B работает как хоткей", async ({ page }) => {
  await seedDb(page, {
    tasks: [], notes: [], settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.getByRole("button", { name: "+ Новая заметка" }).click();

  const editor = noteEditor(page);
  await editor.click();
  await page.keyboard.insertText("hello");
  await page.keyboard.press("ControlOrMeta+a");
  await page.getByTitle("Жирный (Ctrl+B)").click();
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");

  await page.getByTitle("Чек-лист").click();
  await page.keyboard.insertText("пункт списка");
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");

  await page.getByTitle(/Вики-ссылка/).click();
  await page.keyboard.insertText("другая заметка");
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");

  // Курсор сейчас на новой (последней) строке — предыдущие строки больше не
  // "сырые", декорации на них должны быть отрендерены.
  await expect(page.locator(".cm-strong", { hasText: "hello" })).toBeVisible();
  await expect(page.locator(".cm-task-checkbox")).toHaveCount(1);
  await expect(page.locator(".cm-wikilink", { hasText: "другая заметка" })).toBeVisible();

  // Ctrl+B как хоткей: выделяем "hello" (уже **hello**) заново и снимаем жирный.
  await page.keyboard.press("ControlOrMeta+Home");
  await page.keyboard.press("Shift+End");
  await page.keyboard.press("ControlOrMeta+b");
  await page.keyboard.press("ControlOrMeta+Home"); // курсор больше не на этой строке — декорация должна вернуться

  // Сохранённый markdown — источник истины (декорации могут визуально
  // прятать сырые маркеры, textContent() DOM тут ненадёжен).
  await page.waitForTimeout(1000);
  const saved = await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db") || "{}");
    return db.notes?.[0]?.content ?? "";
  });
  expect(saved).toContain("hello"); // жирный снят Ctrl+B, markers should be gone
  expect(saved).not.toContain("**hello**");
  expect(saved).toContain("- [ ] пункт списка");
  expect(saved).toContain("[[другая заметка]]");
});

test("таблицы в заметках: рендерится <table>, ячейка редактируется кликом, +строка/+столбец, курсор внутри блока показывает сырой markdown", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: "n1", title: "заметка с таблицей",
      content: "текст до\n\n| Имя | Возраст |\n| --- | ---: |\n| Аня | 30 |\n| Боб | 7 |\n\nтекст после",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    settings: { onboarding_complete: true },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "заметка с таблицей" }).click();

  await expect(page.locator(".cm-table")).toBeVisible();
  await expect(page.locator(".cm-table th")).toHaveCount(2);
  await expect(page.locator(".cm-table td")).toHaveCount(4);

  // Редактирование ячейки кликом: выделяем весь текст внутри именно этой
  // ячейки (не всего документа — это была реальная регрессия при разработке,
  // contenteditable="false" на обёртке виджета не создаёт отдельный edit-host
  // для Selection API в Chromium) и заменяем.
  await page.locator(".cm-table td", { hasText: "Аня" }).click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText("Оля");
  await page.keyboard.press("Tab"); // коммитит правку и переходит в соседнюю ячейку

  await page.getByRole("button", { name: "+ строка" }).click();
  await page.getByRole("button", { name: "+ столбец" }).click();

  await page.waitForTimeout(1000); // автосейв
  const saved = await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db") || "{}");
    return db.notes?.[0]?.content ?? "";
  });
  expect(saved).toContain("Оля");
  expect(saved).not.toContain("| Аня ");
  expect(saved).toMatch(/\|\s*\|\s*\|\s*\|\s*\n/); // добавленная пустая строка (3 столбца после +столбец)
  expect(saved).toContain("Колонка 3"); // авто-название нового столбца
  expect(saved).toContain("текст до");
  expect(saved).toContain("текст после");

  // Печатание таблицы с нуля: пока курсор на строке заголовка/разделителя,
  // виджет не подменяет текст (иначе редактировать вслепую) — рендерится
  // только когда курсор покидает диапазон строк таблицы.
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  const editor = noteEditor(page);
  await editor.click();
  await page.keyboard.insertText("| A | B |");
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");
  await page.keyboard.insertText("| --- | --- |");
  await expect(page.locator(".cm-table")).toHaveCount(0);
  await page.keyboard.press("End");
  await page.keyboard.insertText("\n");
  await expect(page.locator(".cm-table")).toBeVisible();

  // Кнопка "Таблица" в панели форматирования вставляет стартовую 2x2-таблицу.
  await page.getByRole("button", { name: "+ Новая заметка" }).click();
  await editor.click();
  await page.keyboard.insertText("текст перед вставкой");
  await page.getByTitle("Таблица").click();
  await expect(page.locator(".cm-table")).toBeVisible();
  await expect(page.locator(".cm-table th")).toHaveCount(2);
  await expect(page.locator(".cm-table td")).toHaveCount(4);
});

test("ИИ по выделению в редакторе: меню действий, предпросмотр результата, подтверждение заменяет выделение", async ({ page }) => {
  await seedDb(page, { tasks: [], notes: [], settings: { onboarding_complete: true, ai_provider: "local" } });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.getByRole("button", { name: "+ Новая заметка" }).click();

  const editor = noteEditor(page);
  await editor.click();
  await page.keyboard.insertText("исходный текст для правки");
  await page.keyboard.press("ControlOrMeta+a");

  await expect(page.getByRole("button", { name: "Сократить" })).toBeVisible();
  await page.getByRole("button", { name: "Сократить" }).click();

  const preview = page.locator(".selection-preview");
  await expect(preview).toBeVisible();
  await expect(preview).toHaveText("[shorten] исходный текст для правки");

  await page.getByTitle("Заменить выделение").click();
  await expect(page.locator(".selection-menu")).toHaveCount(0);

  await page.waitForTimeout(1000);
  const saved = await page.evaluate(() => {
    const db = JSON.parse(localStorage.getItem("__mock_db") || "{}");
    return db.notes?.[0]?.content ?? "";
  });
  expect(saved).toBe("[shorten] исходный текст для правки");
});

test("ИИ: резюме заметки — кнопка открывает окно с результатом, клик по тексту копирует и закрывает", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: "n1", title: "Длинная заметка", content: "много текста для резюме",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    settings: { onboarding_complete: true, ai_provider: "local" },
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "Длинная заметка" }).click();

  const summarizeBtn = page.getByTitle("ИИ: резюме заметки");
  await expect(summarizeBtn).toBeVisible();
  await summarizeBtn.click();

  const summaryText = page.locator(".summary-text");
  await expect(summaryText).toBeVisible();
  await expect(summaryText).toContainText("Пункт резюме");

  await summaryText.click();
  await expect(page.locator(".summary-dialog")).toHaveCount(0);
});

test("экспорт заметки в HTML: кнопка сохраняет самодостаточный HTML-файл с заголовком и контентом", async ({ page }) => {
  await seedDb(page, {
    tasks: [],
    notes: [{
      id: "n1", title: "Заметка для экспорта", content: "# Заголовок\n\nТекст с **жирным** и [[wiki-ссылкой]].",
      tags: [], linked_task_id: null, project_id: null,
      created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
    }],
    settings: { onboarding_complete: true },
    mockDialogPath: "/mock/export/note.html",
  });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.locator(".note-item", { hasText: "Заметка для экспорта" }).click();
  await page.getByTitle("Экспорт в HTML").click();

  await expect.poll(async () => {
    const raw = await page.evaluate(() => localStorage.getItem("__mock_db"));
    return raw ? JSON.parse(raw).exportedHtml?.path : null;
  }).toBe("/mock/export/note.html");
  const db = JSON.parse(await page.evaluate(() => localStorage.getItem("__mock_db")!));
  const html = db.exportedHtml.html as string;
  expect(html).toContain("<title>Заметка для экспорта</title>");
  expect(html).toContain("<h1>Заголовок</h1>");
  expect(html).toContain("<strong>жирным</strong>");
  expect(html).toContain("wiki-ссылкой");
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
  await page.locator(".settings-tab", { hasText: "Данные" }).click();
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

  // v0.8.3: шаблоны перенесены из панели строки в модалку задачи.
  await page.getByRole("button", { name: "+ Новая", exact: true }).click();
  const modal = page.locator(".modal");
  await modal.getByPlaceholder("Название задачи").fill("поездка");
  const draft = modal.locator(".check-input").last();
  await draft.fill("паспорт");
  await draft.press("Enter");
  await modal.locator(".check-input").last().fill("билеты");
  await modal.locator(".check-input").last().press("Enter");
  await expect(modal.locator(".check-input")).toHaveCount(3); // паспорт, билеты, пустая заготовка

  // Сохраняем как шаблон прямо из модалки создания
  await modal.getByRole("button", { name: "Сохранить как шаблон" }).click();
  await modal.getByPlaceholder("Название шаблона").fill("Поездка");
  await modal.getByRole("button", { name: "Сохранить", exact: true }).click();

  await modal.getByRole("button", { name: "Создать" }).click();
  await createTask(page, "другая задача");

  // Применяем шаблон к «другой задаче»
  const otherRow = page.locator(".task-row", { hasText: "другая задача" });
  await otherRow.locator(".task-main").click();
  const otherModal = page.locator(".modal");
  await otherModal.getByRole("button", { name: "Из шаблона…" }).click();
  await expect(otherModal.getByText("Поездка")).toBeVisible();
  await otherModal.getByRole("button", { name: "Применить" }).click();
  await expect(otherModal.locator(".check-input")).toHaveCount(3);
  await otherModal.getByRole("button", { name: "Сохранить", exact: true }).click();

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

test("пресет цветов: задаёт основной и дополнительный акцент, переживает перезагрузку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByRole("button", { name: "Закат" }).click();

  const accent = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim());
  const secondary = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent-secondary").trim());
  expect(accent).toBe("#f43f5e");
  expect(secondary).toBe("#f59e0b");

  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await page.reload();

  const accentAfter = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim());
  const secondaryAfter = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent-secondary").trim());
  expect(accentAfter).toBe("#f43f5e");
  expect(secondaryAfter).toBe("#f59e0b");
});

test("настройки: ИИ-провайдер — выпадающий список переключает поля, сохранение работает", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByRole("tab", { name: "ИИ", exact: true }).click();
  const providerSelect = page.locator("section", { hasText: "ИИ-провайдер" }).locator("select").first();

  await expect(page.getByPlaceholder("sk-...")).toHaveCount(0);
  await expect(page.getByPlaceholder("sk-ant-...")).toHaveCount(0);

  await providerSelect.selectOption("openai");
  await expect(page.getByPlaceholder("sk-...")).toBeVisible();
  await expect(page.getByPlaceholder("sk-ant-...")).toHaveCount(0);
  await page.getByPlaceholder("sk-...").fill("sk-test-openai");

  await providerSelect.selectOption("anthropic");
  await expect(page.getByPlaceholder("sk-ant-...")).toBeVisible();
  await expect(page.getByPlaceholder("sk-...")).toHaveCount(0);
  await page.getByPlaceholder("sk-ant-...").fill("sk-ant-test");

  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await page.reload();
  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByRole("tab", { name: "ИИ", exact: true }).click();

  await expect(providerSelect).toHaveValue("anthropic");
  await expect(page.getByPlaceholder("sk-ant-...")).toHaveValue("sk-ant-test");
});

test("настройки: список локальных моделей — карточки с описанием и требованиями, выбор переключает URL для скачивания", async ({ page }) => {
  await seedDb(page, { tasks: [], notes: [], settings: { onboarding_complete: true, ai_provider: "local" } });
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.getByRole("tab", { name: "ИИ", exact: true }).click();

  // Рекомендованная модель выбрана по умолчанию, у неё бейдж "рекомендуется"
  const recommendedOption = page.locator("label", { hasText: "Qwen2.5 1.5B Instruct" });
  await expect(recommendedOption).toBeVisible();
  await expect(recommendedOption.locator("input[type=radio]")).toBeChecked();
  await expect(page.getByText("рекомендуется")).toBeVisible();

  // У каждой модели видно размер, требования по ОЗУ и описание
  await expect(page.getByText(/ГБ · от \d+ ГБ ОЗУ/).first()).toBeVisible();
  await expect(page.getByText("Самая быстрая и лёгкая")).toBeVisible();

  // Выбор другой модели переключает, какая скачается
  const phiOption = page.locator("label", { hasText: "Phi-3.5 Mini Instruct" });
  await phiOption.click();
  await expect(phiOption.locator("input[type=radio]")).toBeChecked();
  await expect(recommendedOption.locator("input[type=radio]")).not.toBeChecked();

  // Свой URL — переключает на custom, поле редактируемое
  await page.getByPlaceholder("https://.../model.gguf").fill("https://example.com/custom.gguf");
  await expect(page.locator("label", { hasText: "Свой URL" }).locator("input[type=radio]")).toBeChecked();
});

test("настройки: хоткеи — переназначение применяется, дефолтная комбинация перестаёт работать", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.locator(".settings-tab", { hasText: "Хоткеи" }).click();
  const hotkeysSection = page.locator("section", { hasText: "Хоткеи" });
  const settingsRow = hotkeysSection.locator(".keybind-row", { hasText: "Перейти: Настройки" });

  await expect(settingsRow.locator(".keybind-combo")).toHaveText("Ctrl+5");

  await settingsRow.locator(".keybind-combo").click();
  await page.keyboard.press("Control+9");
  await expect(settingsRow.locator(".keybind-combo")).toHaveText("Ctrl+9");

  await page.getByRole("button", { name: "Сохранить", exact: true }).click();

  // Без перезагрузки: старая комбинация Ctrl+5 больше не переключает на
  // Настройки, новая Ctrl+9 работает сразу (App.svelte подхватывает
  // сохранённые хоткеи по событию keybinds-saved, не только при onMount).
  await page.getByRole("button", { name: "Задачи" }).click();
  await page.keyboard.press("Control+5");
  await expect(page.getByRole("heading", { name: "Настройки" })).toHaveCount(0);

  await page.keyboard.press("Control+9");
  await expect(page.getByRole("heading", { name: "Настройки" })).toBeVisible();
  await page.locator(".settings-tab", { hasText: "Хоткеи" }).click();

  // Сброс к дефолту возвращает Ctrl+5 — тоже без reload
  const settingsRowAfter = page.locator("section", { hasText: "Хоткеи" }).locator(".keybind-row", { hasText: "Перейти: Настройки" });
  await settingsRowAfter.getByTitle("Сбросить к дефолту").click();
  await expect(settingsRowAfter.locator(".keybind-combo")).toHaveText("Ctrl+5");
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();

  await page.getByRole("button", { name: "Задачи" }).click();
  await page.keyboard.press("Control+9");
  await expect(page.getByRole("heading", { name: "Настройки" })).toHaveCount(0);
  await page.keyboard.press("Control+5");
  await expect(page.getByRole("heading", { name: "Настройки" })).toBeVisible();
});

test("настройки: вкладки показывают только свои секции", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();

  // По умолчанию — «Общее»: видны «Внешний вид» и «Режим работы», остальные скрыты
  await expect(page.locator(".settings-tab.active")).toHaveText("Общее");
  await expect(page.getByText("Внешний вид")).toBeVisible();
  await expect(page.getByText("Режим работы")).toBeVisible();
  await expect(page.getByText("ИИ-провайдер")).toHaveCount(1); // в DOM
  await expect(page.getByText("ИИ-провайдер")).not.toBeVisible();

  await page.getByRole("tab", { name: "ИИ", exact: true }).click();
  await expect(page.getByText("ИИ-провайдер")).toBeVisible();
  await expect(page.getByText("Внешний вид")).not.toBeVisible();

  await page.locator(".settings-tab", { hasText: "Хоткеи" }).click();
  await expect(page.getByText("ИИ-провайдер")).not.toBeVisible();
  await expect(page.locator("section", { hasText: "Хоткеи" })).toBeVisible();
});

test("настройки: поиск скрывает несовпавшие секции и переключает вкладку", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  const sections = page.locator(".settings section");
  await expect(sections).toHaveCount(9); // все секции в DOM независимо от вкладки

  // «бэкап» — совпадение только в секции «Авто-бэкап» (вкладка «Данные»),
  // поиск (v0.8.10) сам переключает на неё.
  await page.getByPlaceholder("Поиск по настройкам…").fill("бэкап");
  await expect(page.locator(".settings-tab.active")).toHaveText("Данные");
  await expect(page.locator(".settings section:visible")).toHaveCount(1);
  await expect(page.locator(".settings section:visible .section-title")).toHaveText("Авто-бэкап");

  // Очистка поиска возвращает все секции активной («Данные») вкладки —
  // без сброса вкладки на «Общее».
  await page.getByPlaceholder("Поиск по настройкам…").fill("");
  await expect(page.locator(".settings-tab.active")).toHaveText("Данные");
  await expect(page.locator(".settings section:visible")).toHaveCount(2); // Авто-бэкап + Данные
});

test("авто-бэкап: секция в настройках, кнопка «Сделать сейчас» вызывает команду", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Настройки" }).click();
  await page.locator(".settings-tab", { hasText: "Данные" }).click();

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
  await page.locator(".settings-tab", { hasText: "Данные" }).click();

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
