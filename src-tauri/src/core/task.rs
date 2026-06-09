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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
                "Done" => TaskStatus::Done,
                "Archived" => TaskStatus::Archived,
                _ => TaskStatus::Todo,
            },
            priority: match self.priority.as_str() {
                "Low" => Priority::Low,
                "High" => Priority::High,
                "Critical" => Priority::Critical,
                _ => Priority::Medium,
            },
            category: match self.category.as_str() {
                "Work" => Category::Work,
                "Study" => Category::Study,
                "Home" => Category::Home,
                "Health" => Category::Health,
                _ => Category::Other,
            },
            deadline: self.deadline.and_then(|d| d.parse().ok()),
            tags: serde_json::from_str(&self.tags).unwrap_or_default(),
            created_at: self.created_at.parse().unwrap(),
            updated_at: self.updated_at.parse().unwrap(),
        }
    }
}