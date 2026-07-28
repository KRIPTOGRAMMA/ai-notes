// Английский словарь (v0.9.32): русский оригинал → перевод.
//
// Ключ — русский текст ровно как в разметке. Отсутствие ключа не ошибка:
// t() вернёт сам ключ, то есть русскую строку. Это позволяет наполнять
// словарь постепенно, не ломая интерфейс.
//
// Порядок — по экранам, чтобы при добавлении строки было видно, куда её
// класть, и чтобы диффы оставались локальными.

export const EN: Record<string, string> = {
  // --- Навигация и общее ---
  "Задачи": "Tasks",
  "Заметки": "Notes",
  "Дашборд": "Dashboard",
  "Календарь": "Calendar",
  "Настройки": "Settings",
  "Сегодня": "Today",
  "Граф заметок": "Notes graph",
  "Граф": "Graph",

  // --- Командная палитра ---
  "Новая задача": "New task",
  "Новая заметка": "New note",
  "Заметка дня": "Daily note",
  "Спланировать день": "Plan the day",
  "Сменить тему": "Switch theme",
  "Перейти: Сегодня": "Go to: Today",
  "Перейти: Задачи": "Go to: Tasks",
  "Перейти: Заметки": "Go to: Notes",
  "Перейти: Граф заметок": "Go to: Notes graph",
  "Перейти: Дашборд": "Go to: Dashboard",
  "Перейти: Календарь": "Go to: Calendar",
  "Перейти: Настройки": "Go to: Settings",
  "Неделя": "Week",
  "Месяц": "Month",
  "Сохранить": "Save",
  "Сохранение...": "Saving...",
  "Сохранено ✓": "Saved ✓",
  "Отмена": "Cancel",
  "Закрыть": "Close",
  "Удалить": "Delete",
  "Восстановить": "Restore",
  "Добавить": "Add",
  "Загрузка…": "Loading…",
  "Думаю…": "Thinking…",
  "Все": "All",
  "Готово!": "Done!",

  // --- Настройки: вкладки ---
  "Общее": "General",
  "ИИ": "AI",
  "Категории": "Categories",
  "Уведомления": "Notifications",
  "Данные": "Data",
  "Хоткеи": "Hotkeys",
  "Справка": "Help",

  // --- Настройки: внешний вид ---
  "Внешний вид": "Appearance",
  "Тема": "Theme",
  "Системная": "System",
  "Светлая": "Light",
  "Тёмная": "Dark",
  "Акцент": "Accent",
  "Доп. акцент": "Secondary accent",
  "Сбросить цвета": "Reset colors",
  "Язык": "Language",

  // --- Настройки: ИИ ---
  "ИИ-провайдер": "AI provider",
  "Без ИИ (функции отключены)": "No AI (features disabled)",
  "Локальная модель (llamafile)": "Local model (llamafile)",
  "Локальная модель хранится в": "The local model is stored in",

  // --- Настройки: режим работы и мониторинг ---
  "Режим работы": "Work mode",
  "Мониторинг": "Monitoring",
  "Применяется после перезапуска приложения.": "Takes effect after restarting the app.",
  "Разбивать браузерное время по сайтам": "Break down browser time by site",
  "Забыть собранные домены": "Forget collected domains",
  "Очищено записей: {n}": "Rows cleared: {n}",

  // --- Настройки: данные ---
  "Авто-бэкап": "Auto backup",
  "Папка для бэкапов": "Backup folder",
  "Хранить копий": "Keep copies",
  "Сделать сейчас": "Run now",
  "Авто-очистка истории (мес., 0 — выкл)": "Auto-clean history (months, 0 = off)",

  // --- Дашборд ---
  "Активное время": "Active time",
  "Активность по дням (мин)": "Activity by day (min)",
  "Активность по часам (8 недель)": "Activity by hour (8 weeks)",
  "Выполнено по категориям": "Completed by category",
  "Выполненные задачи за год": "Tasks completed this year",
  "Приложения": "Apps",
  "Сайты": "Sites",
  "Время по проектам (7 дней)": "Time by project (7 days)",
  "Время всего": "Total time",
  "за неделю": "this week",

  // --- Задачи ---
  "Активные": "Active",
  "История": "History",
  "Корзина": "Trash",
  "Список": "List",
  "Доска": "Board",
  "+ Новая": "+ New",
  "Проекты": "Projects",
  "Поиск задач…": "Search tasks…",
  "Без проекта": "No project",
  "Все проекты": "All projects",
  "Без дедлайна": "No deadline",
  "Есть дедлайн": "Has deadline",
  "Все теги": "All tags",
  "Выполнить": "Complete",
  "Дедлайн": "Deadline",
  "Дедлайн и повтор": "Deadline and recurrence",
  "Без повтора": "No recurrence",
  "Высокий": "High",
  "Средний": "Medium",
  "Низкий": "Low",
  "Критический": "Critical",
  "Завершена": "Completed",

  // --- Заметки ---
  "+ Новая заметка": "+ New note",
  "Выберите заметку или создайте новую": "Select a note or create a new one",
  "Версии заметки": "Note versions",
  "Выберите версию слева для просмотра": "Select a version on the left to preview",
  "Заметка": "Note",
  "Напоминание": "Reminder",
  "Проект": "Project",
  "Бэклог": "Backlog",

  // --- Быстрый ввод ---
  "Задача": "Task",
  "Название задачи...": "Task title...",
  "Заголовок заметки...": "Note title...",
  "Создать": "Create",
  "Текст из буфера обмена — можно поправить перед сохранением":
    "Text from the clipboard — you can edit it before saving",

  // --- Онбординг ---
  "Добро пожаловать в AI Notes": "Welcome to AI Notes",
  "Автозагрузка и хоткеи": "Autostart and hotkeys",
  "Начать настройку": "Start setup",
  "Далее": "Next",
  "Назад": "Back",
  "Начать": "Start",
  "Без ИИ": "No AI",
};
