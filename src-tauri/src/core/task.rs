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