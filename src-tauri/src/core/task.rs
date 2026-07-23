use serde::{Deserialize, Serialize};
use chrono::{DateTime, Datelike, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
  Low,
  Medium,
  High,
  Critical,
}

// Статус задачи — с v0.9.20 это id строки в таблице statuses (тот же
// приём, что category получила в v0.6.3), а не enum. Todo/InProgress/
// Done/Archived остаются зарезервированными id (см. RESERVED_STATUSES в
// commands::statuses) — с ними завязана бизнес-логика (Done →
// hidden+completed_at, InProgress → тайм-трекинг), но название/цвет можно
// кастомизировать, а пользователь может добавлять свои промежуточные
// статусы для канбан-доски. Валидация — на записи
// (commands::statuses::valid_or_fallback), чтение — как есть.
pub type TaskStatus = String;

// Категория задачи — с v0.6.3 это id строки в таблице categories
// (пользовательские категории), а не enum. Валидация — на записи
// (commands::categories::valid_or_fallback), чтение — как есть.

// Внимание: Task НЕ читается из БД напрямую через FromRow — там разные
// представления enum'ов/Vec, и query_as::<_, Task> никогда не вызывается.
// Чтение всегда идёт через TaskRow -> into_task(). Не добавляйте сюда
// derive(sqlx::FromRow) обратно, это вводит в заблуждение.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
  pub id: String,
  pub title: String,
  pub description: Option<String>,
  pub status: TaskStatus,
  pub priority: Priority,
  pub category: String,
  pub deadline: Option<DateTime<Utc>>,
  pub tags: Vec<String>,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
  pub completed_at: Option<DateTime<Utc>>,
  pub recurrence: Recurrence,
  pub hidden: bool,
  // Мягкое удаление (v0.8.12): задача не показывается в активных/истории,
  // но остаётся в таблице до restore или purge из «Корзины».
  #[serde(default)]
  pub deleted_at: Option<DateTime<Utc>>,
  #[serde(default)]
  pub project_id: Option<String>,
  // Тайм-блок: запланированное время работы (не дедлайн)
  #[serde(default)]
  pub scheduled_at: Option<DateTime<Utc>>,
  #[serde(default)]
  pub scheduled_mins: Option<i64>,
  // Ручной порядок в списке (drag); назначается на создании и в reorder_tasks
  #[serde(default)]
  pub sort_order: i64,
  #[serde(default)]
  pub subtasks: Vec<Subtask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::FromRow)]
pub struct Subtask {
  pub id: String,
  pub task_id: String,
  pub title: String,
  pub done: bool,
  pub position: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    pub category: String,
    pub deadline: Option<String>,
    pub tags: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub recurrence: String,
    pub hidden: bool,
    pub deleted_at: Option<String>,
    pub project_id: Option<String>,
    pub scheduled_at: Option<String>,
    pub scheduled_mins: Option<i64>,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateTask {
  pub title: String,
  pub description: Option<String>,
  pub status: TaskStatus,
  pub priority: Priority,
  pub category: String,
  pub deadline: Option<DateTime<Utc>>,
  pub tags: Vec<String>,
  pub recurrence: Option<Recurrence>,
  #[serde(default)]
  pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub category: Option<String>,
    pub deadline: Option<String>,
    pub tags: Option<Vec<String>>,
    pub recurrence: Option<Recurrence>,
    // Как deadline: пустая строка = отвязать от проекта, отсутствие = не менять
    pub project_id: Option<String>,
    // Тайм-блок: пустая строка scheduled_at снимает блок (и длительность)
    pub scheduled_at: Option<String>,
    pub scheduled_mins: Option<i64>,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum RecurrenceUnit {
    Minutes,
    Hours,
    Days,
    Weeks,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Recurrence {
    None,
    Hourly,
    Daily,
    Weekly,
    Custom(u32, RecurrenceUnit),
    // Битовая маска дней недели: бит 0 = Пн ... бит 6 = Вс (тот же паттерн,
    // что days_mask у routines). Не подходит для to_duration — следующая
    // дата не фиксированный интервал, см. next_occurrence.
    Weekdays(u8),
}

impl Recurrence {
    // Парсим из строки БД
    pub fn from_db(s: &str) -> Self {
        match s {
            "Hourly" => Recurrence::Hourly,
            "Daily"  => Recurrence::Daily,
            "Weekly" => Recurrence::Weekly,
            _ if s.starts_with("Custom") => {
                // Формат: "Custom(4,Hours)"
                let inner = s
                    .trim_start_matches("Custom(")
                    .trim_end_matches(')');
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() == 2 {
                    let n = parts[0].trim().parse::<u32>().unwrap_or(1);
                    let unit = match parts[1].trim() {
                        "Minutes" => RecurrenceUnit::Minutes,
                        "Hours"   => RecurrenceUnit::Hours,
                        "Weeks"   => RecurrenceUnit::Weeks,
                        _         => RecurrenceUnit::Days,
                    };
                    Recurrence::Custom(n, unit)
                } else {
                    Recurrence::None
                }
            }
            _ if s.starts_with("Weekdays") => {
                // Формат: "Weekdays(37)"
                let inner = s.trim_start_matches("Weekdays(").trim_end_matches(')');
                match inner.trim().parse::<u8>() {
                    Ok(mask) if mask != 0 => Recurrence::Weekdays(mask),
                    _ => Recurrence::None,
                }
            }
            _ => Recurrence::None,
        }
    }

    // Сохраняем в строку для БД
    pub fn to_db(&self) -> String {
        match self {
            Recurrence::None           => "None".into(),
            Recurrence::Hourly         => "Hourly".into(),
            Recurrence::Daily          => "Daily".into(),
            Recurrence::Weekly         => "Weekly".into(),
            Recurrence::Custom(n, u)   => format!("Custom({},{:?})", n, u),
            Recurrence::Weekdays(mask) => format!("Weekdays({})", mask),
        }
    }

    // Возвращает Duration для пересчёта дедлайна
    pub fn to_duration(&self) -> Option<chrono::Duration> {
        match self {
            Recurrence::None              => None,
            Recurrence::Hourly            => Some(chrono::Duration::hours(1)),
            Recurrence::Daily             => Some(chrono::Duration::days(1)),
            Recurrence::Weekly            => Some(chrono::Duration::weeks(1)),
            Recurrence::Custom(n, unit)   => {
                let n = *n as i64;
                Some(match unit {
                    RecurrenceUnit::Minutes => chrono::Duration::minutes(n),
                    RecurrenceUnit::Hours   => chrono::Duration::hours(n),
                    RecurrenceUnit::Days    => chrono::Duration::days(n),
                    RecurrenceUnit::Weeks   => chrono::Duration::weeks(n),
                })
            }
            Recurrence::Weekdays(_)       => None, // не фиксированный интервал, см. next_occurrence
        }
    }

    // Следующая дата выполнения для recurring-задачи, единая точка для всех
    // вариантов (включая Weekdays, для которого to_duration неприменим).
    // None — не recurring (то же условие, что и у to_duration == None).
    pub fn next_occurrence(&self, from: chrono::DateTime<chrono::Utc>) -> Option<chrono::DateTime<chrono::Utc>> {
        if let Recurrence::Weekdays(mask) = self {
            if *mask == 0 { return None; }
            // Ищем ближайший будущий день недели из маски: от +1 дня (никогда
            // не "сегодня же" — иначе завершение задачи в её же день недели
            // не двигало бы дедлайн вперёд) до +7 дней (маска непуста -> найдётся).
            for delta in 1..=7 {
                let candidate = from + chrono::Duration::days(delta);
                let weekday_bit = 1u8 << candidate.weekday().num_days_from_monday();
                if mask & weekday_bit != 0 {
                    return Some(candidate);
                }
            }
            return None; // недостижимо при mask != 0, но без паники на всякий случай
        }
        self.to_duration().map(|d| from + d)
    }
}

impl CreateTask {
    pub fn into_task(self) -> Task {
        Task {
            id: Uuid::new_v4().to_string(),
            title: self.title,
            description: self.description,
            status: self.status,
            priority: self.priority,
            category: self.category,
            deadline: self.deadline,
            tags: self.tags,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            recurrence: self.recurrence.unwrap_or(Recurrence::None),
            hidden: false,
            deleted_at: None,
            project_id: self.project_id.filter(|p| !p.is_empty()),
            scheduled_at: None,
            scheduled_mins: None,
            sort_order: 0, // create_task_impl назначает max+1 перед вставкой
            subtasks: Vec::new(),
        }
    }
}

impl TaskRow {
    pub fn into_task(self) -> Task {
        Task {
            id: self.id,
            title: self.title,
            description: self.description,
            status: self.status,
            priority: match self.priority.as_str() {
                "Low"      => Priority::Low,
                "High"     => Priority::High,
                "Critical" => Priority::Critical,
                _          => Priority::Medium,
            },
            category: self.category,
            deadline: self.deadline
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            tags: serde_json::from_str(&self.tags).unwrap_or_default(),
            created_at: self.created_at.parse().unwrap_or_else(|_| Utc::now()),
            updated_at: self.updated_at.parse().unwrap_or_else(|_| Utc::now()),
            completed_at: self.completed_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            recurrence: Recurrence::from_db(&self.recurrence),
            hidden: self.hidden,
            deleted_at: self.deleted_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            project_id: self.project_id,
            scheduled_at: self.scheduled_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            scheduled_mins: self.scheduled_mins,
            sort_order: self.sort_order,
            subtasks: Vec::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn row(overrides: impl FnOnce(&mut TaskRow)) -> TaskRow {
        let mut r = TaskRow {
            id: "id-1".into(),
            title: "t".into(),
            description: None,
            status: "Todo".into(),
            priority: "Medium".into(),
            category: "Work".into(),
            deadline: None,
            tags: "[]".into(),
            created_at: "2026-07-06T10:00:00+00:00".into(),
            updated_at: "2026-07-06T10:00:00+00:00".into(),
            completed_at: None,
            recurrence: "None".into(),
            hidden: false,
            deleted_at: None,
            project_id: None,
            scheduled_at: None,
            scheduled_mins: None,
            sort_order: 0,
        };
        overrides(&mut r);
        r
    }

    #[test]
    fn recurrence_db_roundtrip() {
        let cases = [
            Recurrence::None,
            Recurrence::Hourly,
            Recurrence::Daily,
            Recurrence::Weekly,
            Recurrence::Custom(4, RecurrenceUnit::Hours),
            Recurrence::Custom(90, RecurrenceUnit::Minutes),
            Recurrence::Custom(2, RecurrenceUnit::Weeks),
            Recurrence::Custom(10, RecurrenceUnit::Days),
            Recurrence::Weekdays(0b0000101), // Пн, Ср
            Recurrence::Weekdays(0b1111111), // все дни
        ];
        for rec in cases {
            assert_eq!(Recurrence::from_db(&rec.to_db()), rec);
        }
    }

    #[test]
    fn recurrence_from_db_garbage_is_none() {
        assert_eq!(Recurrence::from_db("abrakadabra"), Recurrence::None);
        assert_eq!(Recurrence::from_db(""), Recurrence::None);
        assert_eq!(Recurrence::from_db("Custom(broken"), Recurrence::None);
        assert_eq!(Recurrence::from_db("Weekdays(0)"), Recurrence::None); // пустая маска — не recurring
        assert_eq!(Recurrence::from_db("Weekdays(broken"), Recurrence::None);
    }

    #[test]
    fn recurrence_to_duration() {
        assert_eq!(Recurrence::None.to_duration(), None);
        assert_eq!(Recurrence::Daily.to_duration(), Some(chrono::Duration::days(1)));
        assert_eq!(
            Recurrence::Custom(90, RecurrenceUnit::Minutes).to_duration(),
            Some(chrono::Duration::minutes(90))
        );
        // Weekdays — не фиксированный интервал, to_duration не подходит (см. next_occurrence)
        assert_eq!(Recurrence::Weekdays(0b1111111).to_duration(), None);
    }

    // --- next_occurrence: v0.8.13, повторы по дням недели ---

    fn utc_ymd(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn weekdays_none_mask_is_not_recurring() {
        assert_eq!(Recurrence::Weekdays(0).next_occurrence(utc_ymd(2026, 7, 20)), None);
    }

    #[test]
    fn weekdays_next_occurrence_never_same_day_even_if_today_matches() {
        // 2026-07-20 — понедельник (бит 0). Маска включает Пн — но следующий
        // раз должен быть через 7 дней, а не "сегодня же".
        let monday = utc_ymd(2026, 7, 20);
        let mask_monday = 0b0000001;
        assert_eq!(
            Recurrence::Weekdays(mask_monday).next_occurrence(monday),
            Some(monday + chrono::Duration::days(7))
        );
    }

    #[test]
    fn weekdays_single_day_mask_finds_nearest_within_week() {
        // Маска только на пятницу (бит 4); сегодня — понедельник -> через 4 дня.
        let monday = utc_ymd(2026, 7, 20);
        let mask_friday = 1u8 << 4;
        assert_eq!(
            Recurrence::Weekdays(mask_friday).next_occurrence(monday),
            Some(monday + chrono::Duration::days(4))
        );
    }

    #[test]
    fn weekdays_mask_spanning_weekend_picks_nearest_bit() {
        // Маска Сб+Вс (биты 5,6); сегодня — пятница -> завтра (суббота).
        let friday = utc_ymd(2026, 7, 24);
        let mask_weekend = (1u8 << 5) | (1u8 << 6);
        assert_eq!(
            Recurrence::Weekdays(mask_weekend).next_occurrence(friday),
            Some(friday + chrono::Duration::days(1))
        );
    }

    #[test]
    fn non_weekdays_next_occurrence_matches_to_duration() {
        let now = utc_ymd(2026, 7, 20);
        assert_eq!(Recurrence::None.next_occurrence(now), None);
        assert_eq!(Recurrence::Daily.next_occurrence(now), Some(now + chrono::Duration::days(1)));
    }

    #[test]
    fn task_row_parses_valid_fields() {
        let task = row(|r| {
            r.status = "InProgress".into();
            r.priority = "Critical".into();
            r.category = "Health".into();
            r.tags = r#"["a","b"]"#.into();
            r.deadline = Some("2026-12-31T23:59:00+00:00".into());
        })
        .into_task();

        assert_eq!(task.status, "InProgress");
        assert_eq!(task.priority, Priority::Critical);
        assert_eq!(task.category, "Health");
        assert_eq!(task.tags, vec!["a", "b"]);
        assert!(task.deadline.is_some());
    }

    #[test]
    fn task_row_falls_back_on_garbage() {
        let task = row(|r| {
            r.status = "???".into();
            r.priority = "???".into();
            r.category = "???".into();
            r.tags = "not json".into();
            r.deadline = Some("not a date".into());
            r.created_at = "not a date".into();
        })
        .into_task();

        // Статус (как и категория с v0.6.3) с v0.9.20 читается как есть —
        // валидация происходит на записи (valid_or_fallback), не на чтении.
        assert_eq!(task.status, "???");
        assert_eq!(task.priority, Priority::Medium);
        // Категория с v0.6.3 читается как есть (валидация — на записи, по таблице)
        assert_eq!(task.category, "???");
        assert!(task.tags.is_empty());
        assert_eq!(task.deadline, None);
        // Битая дата не роняет парсинг — подставляется "сейчас"
        assert!(task.created_at <= Utc::now());
    }
}
