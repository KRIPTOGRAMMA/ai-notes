use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
  Low,
  Medium,
  High,
  Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
  Todo,
  InProgress,
  Done,
  Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum RecurrenceUnit {
    Minutes,
    Hours,
    Days,
    Weeks,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
        }
    }
}