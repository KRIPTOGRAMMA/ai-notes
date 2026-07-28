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
  "Создать задачу": "Create a task",
  "Создать заметку": "Create a note",
  "Открыть/создать дневную заметку": "Open/create the daily note",
  "Календарь-неделя + ИИ-план": "Week calendar + AI plan",
  "Светлая → тёмная → системная": "Light → dark → system",
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

  // --- Задачи: шапка, фильтры, массовые действия (v0.9.36) ---
  "{active} актив. · {history} в истории": "{active} active · {history} in history",
  "Фильтр по проекту": "Filter by project",
  "ИИ посоветует, чем заняться сейчас — по блокам, дедлайнам и приоритетам":
    "AI will suggest what to work on now — based on blocks, deadlines and priorities",
  "Выполнить задачу": "Complete task",
  "Приоритет": "Priority",
  "Переформулировать в SMART": "Rewrite as SMART",
  "Разбить на подзадачи": "Break into subtasks",
  "Авто-категория": "Auto category",
  "ИИ предлагает подзадачи": "AI suggests subtasks",
  "Принять все": "Accept all",
  "+ Добавить": "+ Add",
  "+ подзадача (Enter)": "+ subtask (Enter)",
  "Идёт трекинг": "Tracking in progress",
  "Скрыть": "Hide",
  "Снять выбор": "Clear selection",
  "Перенести в проект": "Move to project",
  "В проект…": "To project…",
  "Перенести": "Move",
  "Сменить категорию": "Change category",
  "Категория…": "Category…",
  "Применить": "Apply",
  "Результаты поиска": "Search results",
  "Ничего не найдено": "Nothing found",
  "Нет активных задач.": "No active tasks.",
  "Создайте первую: «+ Новая» или Ctrl+Shift+N": "Create the first one: “+ New” or Ctrl+Shift+N",
  "История пуста": "History is empty",
  "Корзина пуста": "Trash is empty",
  "Удалить навсегда": "Delete permanently",
  "Пусто": "Empty",
  "+ Колонка": "+ Column",
  "Название статуса": "Status name",
  "Сегодня:": "Today:",
  "Быстрая задача… (!приоритет @категория #тег, завтра 15:00 — Shift+Enter подзадача, Ctrl+Enter создать)":
    "Quick task… (!priority @category #tag, tomorrow 15:00 — Shift+Enter subtask, Ctrl+Enter create)",
  "✓ Выполненные задачи. Повторяющиеся не попадают сюда — они остаются активными.":
    "✓ Completed tasks. Recurring ones don’t land here — they stay active.",
  "🗑 Удалённые задачи. Восстановить можно в любой момент, пока не нажато «Удалить навсегда».":
    "🗑 Deleted tasks. You can restore them any time until “Delete permanently” is pressed.",

  // --- Задачи: проекты ---
  "Удалить проект (задачи останутся без проекта)": "Delete project (tasks will keep no project)",
  "Цель:": "Goal:",
  "задач ·": "tasks ·",
  "мин в": "min per",
  "неделю": "week",
  "месяц": "month",
  "Нет записей": "No records",
  "Проектов пока нет — создайте первый.": "No projects yet — create the first one.",
  "Название нового проекта": "New project name",
  "— без проекта —": "— no project —",

  // --- Задачи: умные списки ---
  "+ Список": "+ List",
  // Названия встроенных умных списков задаются кодом (BUILTIN_SMART_LISTS),
  // поэтому переводятся в месте отрисовки, а не в самой константе.
  "Просроченные": "Overdue",
  "На этой неделе": "This week",
  "Создать умный список": "Create a smart list",
  "Новый умный список": "New smart list",
  "Удалить список": "Delete list",
  "Название": "Name",
  "Например: Важное": "For example: Important",
  "Категория": "Category",
  "Любая": "Any",
  "Любой": "Any",
  "Тег": "Tag",
  "без #": "without #",
  "Не важно": "Doesn’t matter",
  "Условия комбинируются через «И» — задача должна подойти под все заданные.":
    "Conditions combine with “AND” — a task must match all of them.",

  // --- Заметки: список, редактор, ИИ ---
  "Поиск...": "Search...",
  "Поиск…": "Search…",
  "Нет заметок": "No notes",
  "Нет заметок по фильтру": "No notes match the filter",
  "Удалить заметку": "Delete note",
  "Экспорт в HTML": "Export to HTML",
  "Резюме заметки": "Note summary",
  "Сжимаю заметку…": "Summarizing the note…",
  "Скопировать и закрыть": "Copy and close",
  "ИИ: резюме заметки": "AI: summarize note",
  "ИИ: извлечь задачи из заметки": "AI: extract tasks from note",
  "Задачи из заметки:": "Tasks from the note:",
  "Задач в заметке не найдено": "No tasks found in the note",
  "ИИ предложит заметки для связи": "AI will suggest notes to link",
  "Связей не найдено": "No links found",
  "Ссылаются сюда:": "Linked from here:",
  "Связанные:": "Related:",
  "Заголовок": "Heading",
  "Начните писать... (Markdown, чек-листы: - [ ] пункт, ссылки: [[заметка]])":
    "Start typing... (Markdown, checklists: - [ ] item, links: [[note]])",
  "Ещё нет сохранённых версий — они появляются при правках с интервалом от 10 минут.":
    "No saved versions yet — they appear after edits at least 10 minutes apart.",
  "Задача:": "Task:",
  "Проект:": "Project:",
  "Напоминание:": "Reminder:",
  "Убрать напоминание": "Remove reminder",
  "— не привязана —": "— not linked —",
  "+ тег": "+ tag",
  "Сохранение…": "Saving…",

  // --- Заметки: панель форматирования ---
  "Жирный (Ctrl+B)": "Bold (Ctrl+B)",
  "Курсив (Ctrl+I)": "Italic (Ctrl+I)",
  "Вики-ссылка (Ctrl+Shift+K)": "Wiki link (Ctrl+Shift+K)",
  "Чек-лист": "Checklist",
  "Нумерованный список": "Numbered list",
  "Цитата": "Quote",
  "Код": "Code",
  "Таблица": "Table",
  "Ссылка": "Link",
  "Заменить выделение": "Replace selection",

  // --- Календарь (v0.9.37) ---
  "Все активные задачи уже в расписании": "All active tasks are already scheduled",
  "ИИ разложит важные задачи из бэклога по свободному времени сегодня":
    "AI will fit important backlog tasks into today’s free time",
  "Задачи разложены по дате дедлайна. Красные — просроченные, зачёркнутые — выполненные. Клик по задаче открывает её, клик по дню — создаёт задачу с дедлайном на этот день.":
    "Tasks are laid out by deadline date. Red are overdue, struck through are completed. Clicking a task opens it; clicking a day creates a task due that day.",
  "Перетащите на день и время": "Drag onto a day and time",
  "Снять блок": "Remove block",
  "Создать задачу на этот день": "Create a task for this day",
  "Простой внутри блока по данным мониторинга": "Idle time inside the block, from monitoring data",
  "Рутины": "Routines",
  "сегодня": "today",

  // --- Дашборд (v0.9.37) ---
  "ИИ-инсайт": "AI insight",
  "ИИ отключён — включите провайдера в Настройках, чтобы получать инсайты.":
    "AI is off — enable a provider in Settings to get insights.",
  "ИИ отключён — резюме недоступно.": "AI is off — summary unavailable.",
  "Нажмите «Обновить», чтобы получить короткий разбор вашей продуктивности.":
    "Press “Refresh” to get a short breakdown of your productivity.",
  "Резюме": "Summary",
  "Резюме дня или недели: что сделано и сколько времени было активным.":
    "Summary of the day or week: what got done and how much time was active.",
  "Выполненные задачи по категориям": "Completed tasks by category",
  "Нет выполненных задач": "No completed tasks",
  "Нет выполненных задач в этот день": "No tasks completed on this day",
  "Нет данных": "No data",
  "нет данных": "no data",
  "День": "Day",
  "{pct}% актив · {mins} мин": "{pct}% active · {mins} min",
  "{n} дн.": "{n} d",
  "Категории — по правилам «класс окна → категория» в Настройках → Мониторинг.":
    "Categories follow the “window class → category” rules in Settings → Monitoring.",
  "Цели проектов": "Project goals",
  "Минуты — по трекингу задач проекта за период.":
    "Minutes come from tracking the project’s tasks over the period.",
  "Помодоро": "Pomodoro",
  "стрик задач": "task streak",
  "стрик помидоров": "pomodoro streak",
  "всего": "total",
  "дней": "days",
  "задачи": "tasks",
  "минут": "minutes",
  "минуты": "minutes",
  "часов": "hours",
  "недель": "weeks",

  // --- Модалка задачи (v0.9.37) ---
  "Редактировать задачу": "Edit task",
  "Первое срабатывание": "First occurrence",
  "Название *": "Name *",
  "Название задачи": "Task title",
  "Описание": "Description",
  "Описание (необязательно)": "Description (optional)",
  "Статус": "Status",
  "Повтор": "Recurrence",
  "Каждый час": "Hourly",
  "Каждый день": "Daily",
  "Каждую неделю": "Weekly",
  "Свой интервал": "Custom interval",
  "По дням недели": "By weekdays",
  "Каждые": "Every",
  "При выполнении задача не закрывается — дедлайн сам сдвинется на следующий срок, задача останется активной.":
    "Completing it doesn’t close the task — the deadline shifts to the next occurrence and the task stays active.",
  "Подзадачи": "Subtasks",
  "Удалить подзадачу": "Delete subtask",
  "Из шаблона…": "From template…",
  "Сохранить как шаблон": "Save as template",
  "Название шаблона": "Template name",
  "Нет сохранённых шаблонов": "No saved templates",
  "Удалить шаблон": "Delete template",
  "Теги (через запятую)": "Tags (comma-separated)",
  "работа, важное, срочное": "work, important, urgent",

  // --- Быстрый ввод (v0.9.37) ---
  "Заголовок...": "Title...",
  "Описание...": "Description...",
  "Текст заметки... (Ctrl+Enter — сохранить)": "Note text... (Ctrl+Enter to save)",
  "Текст... (Ctrl+Enter — сохранить)": "Text... (Ctrl+Enter to save)",
  "Сохранено": "Saved",
  "⚡ Слот пуст": "⚡ Slot is empty",
  "Закрепите задачу или заметку кнопкой-молнией в списке — этот хоткей будет открывать её сразу на правку.":
    "Pin a task or note with the lightning button in the list — this hotkey will open it straight for editing.",
  "закрыть": "close",
  "сохранить ·": "save ·",

  // --- Редактор заметок (v0.9.38) ---
  "Открыть заметку": "Open note",
  "Редактировать": "Edit",

  // --- Рутины (v0.9.38) ---
  "+ Добавить рутину": "+ Add routine",
  "Название рутины": "Routine name",
  "Начало": "Start",
  "Длительность (мин)": "Duration (min)",

  // --- Экран «Сегодня» (v0.9.38) ---
  "{done} из {total} выполнено": "{done} of {total} done",
  "Блоки на сегодня": "Today’s blocks",
  "На сегодня блоков не запланировано.": "No blocks scheduled for today.",
  "Дедлайны сегодня и просрочка": "Due today and overdue",
  "Ничего срочного.": "Nothing urgent.",

  // --- Помодоро и трекинг (v0.9.38) ---
  "Начать помидор": "Start pomodoro",
  "Пропустить фазу": "Skip phase",
  "Остановить трекинг": "Stop tracking",
  "Остановить": "Stop",

  // --- Поиск, уведомления, модель (v0.9.38) ---
  "Поиск задач и заметок...": "Search tasks and notes...",
  "Уведомлений пока не было": "No notifications yet",
  "Очистить": "Clear",
  "Модель не найдена": "Model not found",
  "Свой URL (GGUF)": "Custom URL (GGUF)",
  "рекомендуется": "recommended",

  // --- Граф заметок и карточка истории (v0.9.38) ---
  "Перетаскивайте узлы, двойной клик — открыть заметку. Приглушённые узлы без связей.":
    "Drag the nodes; double-click opens a note. Dimmed nodes have no links.",
  "Пока нет заметок — граф появится, когда будут заметки со связями [[как эта]].":
    "No notes yet — the graph appears once notes have [[links like this]].",
  "Создана": "Created",
  "Теги": "Tags",

  // --- Онбординг (v0.9.38) ---
  "Задачи, заметки и мониторинг активности — всё локально, приватно и с опциональным ИИ.":
    "Tasks, notes and activity monitoring — all local, private, with optional AI.",
  "Пара минут настройки — и можно работать.": "A couple of minutes of setup and you’re ready.",
  "ИИ-помощник": "AI assistant",
  "ИИ переписывает задачи в SMART-формат, генерирует подзадачи и классифицирует их.":
    "AI rewrites tasks in SMART format, generates subtasks and classifies them.",
  "Можно включить позже в Настройках": "You can enable it later in Settings",
  "Локальная модель": "Local model",
  "Приватно, работает оффлайн. GGUF-модель можно скачать прямо здесь.":
    "Private, works offline. The GGUF model can be downloaded right here.",
  "Облачный API": "Cloud API",
  "OpenAI или Anthropic — API-ключ вводится в Настройках":
    "OpenAI or Anthropic — the API key is entered in Settings",
  "Мониторинг на Wayland": "Monitoring on Wayland",
  "Активность отслеживается системно: композитор сам сообщает о простое и возврате (протокол":
    "Activity is tracked at the system level: the compositor itself reports idle and return (the",
  "). Настраивать ничего не нужно, содержимое ввода приложению не видно — только факт активности.":
    " protocol). Nothing to configure, and the app never sees what you type — only that you were active.",
  "Если композитор не поддерживает протокол, трекинг работает только при окне в фокусе. Текущий режим виден в Настройках → Мониторинг.":
    "If the compositor doesn’t support the protocol, tracking only works while the window is focused. The current mode is shown in Settings → Monitoring.",
  "Запускать AI Notes при входе в систему": "Launch AI Notes at login",
  "Быстрая задача из любого места:": "Quick task from anywhere:",
  "На Hyprland/Sway глобальные хоткеи перехватывает композитор — добавь бинд, запускающий":
    "On Hyprland/Sway the compositor intercepts global hotkeys — add a bind that runs",
  "— создание через кнопку или": "— create with the button or",
  "— активность и выполненные задачи по дням": "— activity and completed tasks by day",
  "Трей": "Tray",
  "— быстрое переключение режима (Focus — без уведомлений, Study — помодоро)":
    "— quick mode switching (Focus — no notifications, Study — pomodoro)",
  "Остальное — в": "Everything else is in",
  "Настройках → Справка": "Settings → Help",
  ": там собрано, что умеют заметки, задачи, быстрый ввод, ИИ и мониторинг.":
    ": it covers what notes, tasks, quick capture, AI and monitoring can do.",

  // --- Онбординг ---
  "Добро пожаловать в AI Notes": "Welcome to AI Notes",
  "Автозагрузка и хоткеи": "Autostart and hotkeys",
  "Начать настройку": "Start setup",
  "Далее": "Next",
  "Назад": "Back",
  "Начать": "Start",
  "Без ИИ": "No AI",
};
