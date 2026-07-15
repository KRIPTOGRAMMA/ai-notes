use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
  Low,
  Medium,
  High,
  Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
  Todo,
  InProgress,
  Done,
  Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Category {
  Work,
  Study,
  Home,
  Health,
  Other,
}

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
  pub category: Category,
  pub deadline: Option<DateTime<Utc>>,
  pub tags: Vec<String>,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
  pub completed_at: Option<DateTime<Utc>>,
  pub recurrence: Recurrence,
  pub hidden: bool,
  #[serde(default)]
  pub project_id: Option<String>,
  // Тайм-блок: запланированное время работы (не дедлайн)
  #[serde(default)]
  pub scheduled_at: Option<DateTime<Utc>>,
  #[serde(default)]
  pub scheduled_mins: Option<i64>,
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
    pub project_id: Option<String>,
    pub scheduled_at: Option<String>,
    pub scheduled_mins: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTask {
  pub title: String,
  pub description: Option<String>,
  pub status: TaskStatus,
  pub priority: Priority,
  pub category: Category,
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
    pub category: Option<Category>,
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
        }
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
            project_id: self.project_id.filter(|p| !p.is_empty()),
            scheduled_at: None,
            scheduled_mins: None,
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
            status: match self.status.as_str() {
                "InProgress" => TaskStatus::InProgress,
                "Done"       => TaskStatus::Done,
                "Archived"   => TaskStatus::Archived,
                _            => TaskStatus::Todo,
            },
            priority: match self.priority.as_str() {
                "Low"      => Priority::Low,
                "High"     => Priority::High,
                "Critical" => Priority::Critical,
                _          => Priority::Medium,
            },
            category: match self.category.as_str() {
                "Work"   => Category::Work,
                "Study"  => Category::Study,
                "Home"   => Category::Home,
                "Health" => Category::Health,
                _        => Category::Other,
            },
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
            project_id: self.project_id,
            scheduled_at: self.scheduled_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc)),
            scheduled_mins: self.scheduled_mins,
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
            project_id: None,
            scheduled_at: None,
            scheduled_mins: None,
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
    }

    #[test]
    fn recurrence_to_duration() {
        assert_eq!(Recurrence::None.to_duration(), None);
        assert_eq!(Recurrence::Daily.to_duration(), Some(chrono::Duration::days(1)));
        assert_eq!(
            Recurrence::Custom(90, RecurrenceUnit::Minutes).to_duration(),
            Some(chrono::Duration::minutes(90))
        );
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

        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.priority, Priority::Critical);
        assert_eq!(task.category, Category::Health);
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

        assert_eq!(task.status, TaskStatus::Todo);
        assert_eq!(task.priority, Priority::Medium);
        assert_eq!(task.category, Category::Other);
        assert!(task.tags.is_empty());
        assert_eq!(task.deadline, None);
        // Битая дата не роняет парсинг — подставляется "сейчас"
        assert!(task.created_at <= Utc::now());
    }
}
