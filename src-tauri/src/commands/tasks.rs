use crate::core::task::{CreateTask, Task};

#[tauri::command]
pub fn create_task(task: CreateTask) -> Result<Task, String> {
    Ok(task.into_task())
}