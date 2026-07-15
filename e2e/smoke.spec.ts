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

  // выполнение — уходит из активных, появляется в истории
  await page.locator(".task-check").click();
  await expect(page.locator(".task-main", { hasText: "переименованная" })).toHaveCount(0);
  await page.getByRole("button", { name: "История" }).click();
  await expect(page.getByText("переименованная задача")).toBeVisible();

  // удаление из истории
  await page.getByTitle("Удалить").click();
  await expect(page.getByText("переименованная задача")).toHaveCount(0);
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

test("заметки: превью рендерит чек-лист, клик правит markdown-исходник", async ({ page }) => {
  await withMock(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Заметки" }).click();
  await page.getByRole("button", { name: "+ Новая заметка" }).click();

  const editor = page.getByPlaceholder(/Начните писать/);
  await editor.fill("план:\n- [ ] первый пункт\n- [ ] второй пункт");

  await page.getByRole("button", { name: "Превью" }).click();
  const boxes = page.locator('input[type="checkbox"]');
  await expect(boxes).toHaveCount(2);
  await expect(boxes.first()).toBeEnabled();
  await boxes.first().check();

  await page.getByRole("button", { name: "Редактировать" }).click();
  await expect(editor).toHaveValue("план:\n- [x] первый пункт\n- [ ] второй пункт");
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
  await expect(page.locator(".goal-item", { hasText: "Спорт" })).toBeVisible();
  await expect(page.locator(".goal-val")).toHaveText("1/1");
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
