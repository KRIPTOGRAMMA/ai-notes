// Локализация бэкенда (v0.9.39).
//
// Схема та же, что на фронте (src/lib/i18n.ts): **ключ — это русский текст**.
// Причины те же и здесь: непереведённая строка деградирует в читаемый
// русский оригинал, а не в «notify.deadline_now», и диффы остаются
// читаемыми глазами. Держать две разные схемы в одном приложении было бы
// хуже, чем переиспользовать доказавшую себя.
//
// Что локализуется: только то, что пользователь реально видит вне окна —
// уведомления, меню трея, строка статуса для waybar. Промпты к модели
// (SYSTEM_* в commands/ai.rs, planner.rs) НЕ трогаются: это инструкции для
// LLM, а не интерфейс; их перевод меняет качество ответов и требует
// проверки на живой модели.
//
// Язык читается из настроек при каждом обращении, а не кэшируется в статике:
// уведомления шлются из фоновых циклов, которые живут всё время работы
// приложения, и после переключения языка в Настройках следующий же пуш
// должен прийти на новом языке — без перезапуска.

use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

// Пустая настройка означает «пользователь не выбирал» — тогда, как и на
// фронте, всё нерусское считается английским. Локаль ОС здесь читается из
// LANG/LC_ALL: navigator.language бэкенду недоступен.
pub fn lang_from_setting(saved: &str) -> Lang {
    match saved.trim() {
        "ru" => Lang::Ru,
        "en" => Lang::En,
        _ => detect_from_env(),
    }
}

fn detect_from_env() -> Lang {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    if raw.to_lowercase().starts_with("ru") { Lang::Ru } else { Lang::En }
}

// Текущий язык интерфейса из БД. Ошибка чтения — не повод падать в фоновом
// цикле: язык определяется по окружению, уведомление всё равно уходит.
pub async fn current_lang(pool: &SqlitePool) -> Lang {
    let saved = crate::commands::settings::get_setting(pool, "language")
        .await
        .unwrap_or_default();
    lang_from_setting(&saved)
}

// Словарь: русский оригинал → английский перевод. Отсутствие ключа не
// ошибка — вернётся сам ключ (русский текст).
fn en(key: &str) -> Option<&'static str> {
    Some(match key {
        // --- Уведомления: дедлайны ---
        "Дедлайн наступил!" => "Deadline reached!",
        "Дедлайн через {n} ч" => "Deadline in {n} h",
        "Дедлайн через {n} мин" => "Deadline in {n} min",
        // --- Уведомления: тайм-блоки ---
        "Начался блок (до {time})" => "Block started (until {time})",
        // --- Уведомления: помодоро ---
        "Помодоро запущено: {n} минут работы" => "Pomodoro started: {n} minutes of work",
        "Перерыв {n} минут — отдохни" => "Break for {n} minutes — take a rest",
        "Перерыв окончен: {n} минут работы" => "Break over: {n} minutes of work",
        // --- Уведомления: сводка, цели, лимиты ---
        "Утренняя сводка" => "Morning digest",
        "{cat}: {mins} мин из {limit} сегодня" => "{cat}: {mins} min of {limit} today",
        // --- Уведомления: активность ---
        "Вы отсутствовали {n} мин. Продолжим задачу «{task}» или сделаем перерыв?" =>
            "You were away for {n} min. Continue the task “{task}” or take a break?",
        "Вы отсутствовали {n} мин. Ближайшая задача: {task}" =>
            "You were away for {n} min. Next up: {task}",
        "Вы отсутствовали {n} мин. С возвращением!" => "You were away for {n} min. Welcome back!",
        // --- Меню трея ---
        "Показать" => "Show",
        "Быстрая задача" => "Quick task",
        "Быстрая заметка" => "Quick note",
        "Пауза уведомлений" => "Pause notifications",
        "Выкл" => "Off",
        "30 минут" => "30 minutes",
        "1 час" => "1 hour",
        "До конца дня" => "Until end of day",
        "Бессрочно" => "Indefinitely",
        "Выход" => "Quit",
        "Режим" => "Mode",
        "Открыть" => "Open",
        "2 часа" => "2 hours",
        "Помодоро: пауза/продолжить" => "Pomodoro: pause/resume",
        "Помодоро: пропустить фазу" => "Pomodoro: skip phase",
        "{base} — осталось {n} мин" => "{base} — {n} min left",
        // --- Строка статуса (waybar) ---
        "БД не найдена" => "Database not found",
        "пауза" => "paused",
        "до {time}" => "until {time}",
        "▶ {task} · {n} мин" => "▶ {task} · {n} min",
        "▶ {task} до {time}" => "▶ {task} until {time}",
        "Трекинг: {task} ({n} мин)" => "Tracking: {task} ({n} min)",
        "Помодоро: {phase}" => "Pomodoro: {phase}",
        "работа" => "work",
        "перерыв" => "break",
        "на паузе" => "paused",
        "Идёт: {task} (до {time})" => "Now: {task} (until {time})",
        "Идёт рутина: {task} (до {time})" => "Routine now: {task} (until {time})",
        "Далее: {task} в {time}" => "Next: {task} at {time}",
        "Далее рутина: {task} в {time}" => "Next routine: {task} at {time}",
        "В работе: {task}" => "In progress: {task}",
        "Задач на сегодня: {n}" => "Tasks due today: {n}",
        " (просрочено: {n})" => " (overdue: {n})",
        "Режим: {mode}" => "Mode: {mode}",
        // --- Контекст для ИИ (v0.9.43) ---
        // Это не интерфейс, а данные, которые уходят в промпт. Язык контекста
        // обязан совпадать с языком промпта: английская инструкция поверх
        // русского контекста не работает — модель отвечает на языке данных.
        "{date}: {n} мин" => "{date}: {n} min",
        "нет данных" => "no data",
        "Активные минуты по дням: {mins}. Выполнено задач за последние дни: {done}. Топ-категория выполненных задач: {cat}." =>
            "Active minutes per day: {mins}. Tasks completed in recent days: {done}. Top category of completed tasks: {cat}.",
        "Период: {label}. Выполнено задач: {done}{titles}. Активное время: {mins} мин. Просрочено сейчас: {overdue}." =>
            "Period: {label}. Tasks completed: {done}{titles}. Active time: {mins} min. Currently overdue: {overdue}.",
        "последние сутки" => "the last 24 hours",
        "последняя неделя" => "the last week",
        "последний месяц" => "the last month",
        "{done}/{total} задач" => "{done}/{total} tasks",
        "{done}/{total} мин" => "{done}/{total} min",
        " Цели проектов: {goals}." => " Project goals: {goals}.",
        "{app} ({n} мин)" => "{app} ({n} min)",
        " Топ приложений: {apps}." => " Top apps: {apps}.",
        "Сейчас {time}." => "It is now {time}.",
        "Идёт блок «{task}» до {time}." => "Block “{task}” is running until {time}.",
        "Следующий блок: {time} «{task}»." => "Next block: {time} “{task}”.",
        "Просрочено: {tasks}." => "Overdue: {tasks}.",
        "{task} (приоритет {prio})" => "{task} (priority {prio})",
        "Активных задач нет." => "There are no active tasks.",
        "Важные задачи: {tasks}." => "Important tasks: {tasks}.",
        "Уведомления: выключены" => "Notifications: off",
        "Уведомления: пауза до {time}" => "Notifications: paused until {time}",
        _ => return None,
    })
}

/// Перевод строки. Ключ — русский оригинал; отсутствующий перевод
/// возвращает ключ, а не пустоту.
pub fn tr(key: &str, lang: Lang) -> String {
    match lang {
        Lang::Ru => key.to_string(),
        Lang::En => en(key).unwrap_or(key).to_string(),
    }
}

/// Перевод с подстановкой `{name}`.
pub fn tr_args(key: &str, lang: Lang, args: &[(&str, String)]) -> String {
    let mut out = tr(key, lang);
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_returns_key_unchanged() {
        assert_eq!(tr("Дедлайн наступил!", Lang::Ru), "Дедлайн наступил!");
        // даже если ключа нет в словаре
        assert_eq!(tr("Строки нет в словаре", Lang::Ru), "Строки нет в словаре");
    }

    #[test]
    fn english_uses_dictionary() {
        assert_eq!(tr("Дедлайн наступил!", Lang::En), "Deadline reached!");
        assert_eq!(tr("Выход", Lang::En), "Quit");
    }

    // Главное свойство схемы «ключ = русский текст»: недоделанный перевод
    // деградирует в читаемую строку, а не в пустоту или имя ключа.
    #[test]
    fn missing_translation_falls_back_to_russian() {
        let missing = "Строка, которой точно нет 12345";
        assert_eq!(tr(missing, Lang::En), missing);
        assert!(!tr(missing, Lang::En).is_empty());
    }

    #[test]
    fn args_substituted_in_both_languages() {
        let args = [("n", "5".to_string())];
        assert_eq!(tr_args("Дедлайн через {n} мин", Lang::Ru, &args), "Дедлайн через 5 мин");
        assert_eq!(tr_args("Дедлайн через {n} мин", Lang::En, &args), "Deadline in 5 min");
    }

    #[test]
    fn unused_arg_is_harmless_missing_stays_literal() {
        assert_eq!(tr_args("Выход", Lang::Ru, &[("x", "1".into())]), "Выход");
        assert_eq!(tr_args("Значение {y}", Lang::Ru, &[]), "Значение {y}");
    }

    #[test]
    fn setting_wins_over_environment() {
        assert_eq!(lang_from_setting("ru"), Lang::Ru);
        assert_eq!(lang_from_setting("en"), Lang::En);
        assert_eq!(lang_from_setting("  ru  "), Lang::Ru);
    }

    // Плейсхолдеры перевода обязаны совпадать с оригиналом: разошедшийся
    // {n} означал бы подстановку в никуда и «{n}» на экране пользователя.
    #[test]
    fn placeholders_match_between_key_and_translation() {
        let keys = [
            "Дедлайн через {n} ч",
            "Дедлайн через {n} мин",
            "Начался блок (до {time})",
            "Помодоро запущено: {n} минут работы",
            "Перерыв {n} минут — отдохни",
            "Перерыв окончен: {n} минут работы",
            "{cat}: {mins} мин из {limit} сегодня",
            "Вы отсутствовали {n} мин. Продолжим задачу «{task}» или сделаем перерыв?",
            "Вы отсутствовали {n} мин. Ближайшая задача: {task}",
            "Вы отсутствовали {n} мин. С возвращением!",
            "до {time}",
            "{base} — осталось {n} мин",
        ];
        let names = |s: &str| {
            let mut v: Vec<String> = Vec::new();
            let mut rest = s;
            while let Some(i) = rest.find('{') {
                if let Some(j) = rest[i..].find('}') {
                    v.push(rest[i + 1..i + j].to_string());
                    rest = &rest[i + j + 1..];
                } else {
                    break;
                }
            }
            v.sort();
            v
        };
        for k in keys {
            let translated = en(k).unwrap_or_else(|| panic!("нет перевода для «{k}»"));
            assert_eq!(names(k), names(translated), "плейсхолдеры разошлись в «{k}»");
        }
    }

    // Ключи, которые реально используются в коде, обязаны быть в словаре.
    // Проверка идёт по исходникам: забыть добавить перевод после правки
    // строки легко, и на английском она молча деградирует в русскую —
    // механизм так задуман, но для уже локализованных мест это баг.
    #[test]
    fn every_used_key_is_translated() {
        let sources = [
            include_str!("status.rs"),
            include_str!("notifier/scheduler.rs"),
            include_str!("notifier/pomodoro.rs"),
            include_str!("monitor/activity.rs"),
            include_str!("lib.rs"),
            // v0.9.43: контекст, уходящий в промпт ИИ, — тот же словарь
            include_str!("commands/ai.rs"),
            include_str!("commands/planner.rs"),
        ];
        let mut missing: Vec<String> = Vec::new();
        for src in sources {
            for (idx, _) in src.match_indices("i18n::tr") {
                let rest = &src[idx..];
                let Some(q1) = rest.find('"') else { continue };
                let after = &rest[q1 + 1..];
                let Some(q2) = after.find('"') else { continue };
                let key = &after[..q2];
                // ключи без кириллицы — не наши (например, имена аргументов)
                if !key.chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)) {
                    continue;
                }
                if en(key).is_none() {
                    missing.push(key.to_string());
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "нет перевода для: {missing:?}");
    }

    #[test]
    fn no_empty_translations() {
        for k in ["Дедлайн наступил!", "Выход", "Показать", "пауза"] {
            assert!(!tr(k, Lang::En).trim().is_empty(), "пустой перевод для «{k}»");
        }
    }
}
