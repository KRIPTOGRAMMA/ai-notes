// Проекты (v0.5 фаза 2): группировка задач с прогрессом done/total.
// Прогресс считается по completed_at (выполненные уходят в историю с hidden=1,
// но у проекта они остаются в счётчике).
//
// Цели (v0.5.5): goal_tasks и/или goal_mins за goal_period (week/month).
// Прогресс за текущий период: задачи по completed_at, минуты — по тайм-блокам,
// которые уже начались (scheduled_at в периоде и <= now). Границы периода —
// локальные (понедельник 00:00 / первое число 00:00), в БД сравниваем в UTC.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::State;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    pub target_date: Option<String>, // RFC3339 или NULL
    pub archived: bool,
    pub created_at: String,
    pub task_total: i64,
    pub task_done: i64,
    pub goal_tasks: Option<i64>,
    pub goal_mins: Option<i64>,
    pub goal_period: String, // "week" | "month"
    pub goal_done_tasks: i64,
    pub goal_done_mins: i64,
    #[serde(skip)]
    pub notified_goal: String, // ключ периода последнего пуша о цели
}

#[derive(Debug, Deserialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub target_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub color: Option<String>,
    // Как deadline у задач: пустая строка = убрать дату, отсутствие = не менять
    pub target_date: Option<String>,
    pub archived: Option<bool>,
    // Цели: 0 или меньше = снять цель, отсутствие = не менять
    pub goal_tasks: Option<i64>,
    pub goal_mins: Option<i64>,
    pub goal_period: Option<String>,
}

// Начало текущего периода в локальном времени, возвращённое как UTC RFC3339
// (для сравнения со строками в БД) — формат совпадает с Utc::now().to_rfc3339().
pub fn period_start(now: DateTime<Utc>, period: &str) -> DateTime<Utc> {
    let local = now.with_timezone(&chrono::Local);
    let date = local.date_naive();
    let start_date = if period == "month" {
        date.with_day(1).unwrap_or(date)
    } else {
        date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64)
    };
    start_date
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default()
        .and_local_timezone(chrono::Local)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now)
}

// Ключ периода для notified_goal: локальная дата его начала — без edge-кейсов
// ISO-недель на границе года.
pub fn period_key(now: DateTime<Utc>, period: &str) -> String {
    period_start(now, period)
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string()
}

#[tauri::command]
pub async fn get_projects(pool: State<'_, SqlitePool>) -> AppResult<Vec<Project>> {
    get_projects_impl(pool.inner()).await
}

pub async fn get_projects_impl(pool: &SqlitePool) -> AppResult<Vec<Project>> {
    get_projects_at(pool, Utc::now()).await
}

pub async fn get_projects_at(pool: &SqlitePool, now: DateTime<Utc>) -> AppResult<Vec<Project>> {
    let week = period_start(now, "week").to_rfc3339();
    let month = period_start(now, "month").to_rfc3339();
    let now_str = now.to_rfc3339();
    // Начало периода зависит от goal_period проекта — выбираем через CASE.
    let rows = sqlx::query(
        "SELECT p.id, p.name, p.color, p.target_date, p.archived, p.created_at,
                p.goal_tasks, p.goal_mins, p.goal_period, p.notified_goal,
                COUNT(t.id) AS task_total,
                COALESCE(SUM(t.completed_at IS NOT NULL), 0) AS task_done,
                COALESCE(SUM(t.completed_at IS NOT NULL
                             AND t.completed_at >= CASE p.goal_period WHEN 'month' THEN ? ELSE ? END), 0)
                    AS goal_done_tasks,
                COALESCE(SUM(CASE WHEN t.scheduled_at IS NOT NULL
                                   AND t.scheduled_at <= ?
                                   AND t.scheduled_at >= CASE p.goal_period WHEN 'month' THEN ? ELSE ? END
                             THEN COALESCE(t.scheduled_mins, 60) ELSE 0 END), 0)
                    AS goal_done_mins
         FROM projects p
         LEFT JOIN tasks t ON t.project_id = p.id
         GROUP BY p.id
         ORDER BY p.archived, p.created_at",
    )
    .bind(&month)
    .bind(&week)
    .bind(&now_str)
    .bind(&month)
    .bind(&week)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| Project {
            id: r.get("id"),
            name: r.get("name"),
            color: r.get("color"),
            target_date: r.get("target_date"),
            archived: r.get("archived"),
            created_at: r.get("created_at"),
            task_total: r.get("task_total"),
            task_done: r.get("task_done"),
            goal_tasks: r.get("goal_tasks"),
            goal_mins: r.get("goal_mins"),
            goal_period: r.get("goal_period"),
            goal_done_tasks: r.get("goal_done_tasks"),
            goal_done_mins: r.get("goal_done_mins"),
            notified_goal: r.get("notified_goal"),
        })
        .collect())
}

#[tauri::command]
pub async fn create_project(
    pool: State<'_, SqlitePool>,
    project: CreateProject,
) -> AppResult<Project> {
    create_project_impl(pool.inner(), project).await
}

pub async fn create_project_impl(pool: &SqlitePool, project: CreateProject) -> AppResult<Project> {
    if project.name.trim().is_empty() {
        return Err("Название проекта не может быть пустым".to_string().into());
    }
    let p = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: project.name.trim().to_string(),
        color: project.color,
        target_date: project.target_date.filter(|d| !d.is_empty()),
        archived: false,
        created_at: chrono::Utc::now().to_rfc3339(),
        task_total: 0,
        task_done: 0,
        goal_tasks: None,
        goal_mins: None,
        goal_period: "week".to_string(),
        goal_done_tasks: 0,
        goal_done_mins: 0,
        notified_goal: String::new(),
    };
    sqlx::query(
        "INSERT INTO projects (id, name, color, target_date, archived, created_at)
         VALUES (?, ?, ?, ?, 0, ?)",
    )
    .bind(&p.id)
    .bind(&p.name)
    .bind(&p.color)
    .bind(&p.target_date)
    .bind(&p.created_at)
    .execute(pool)
    .await?;
    Ok(p)
}

#[tauri::command]
pub async fn update_project(
    pool: State<'_, SqlitePool>,
    id: String,
    patch: UpdateProject,
) -> AppResult<()> {
    update_project_impl(pool.inner(), id, patch).await
}

pub async fn update_project_impl(pool: &SqlitePool, id: String, patch: UpdateProject) -> AppResult<()> {
    if let Some(name) = &patch.name {
        if name.trim().is_empty() {
            return Err("Название проекта не может быть пустым".to_string().into());
        }
        sqlx::query("UPDATE projects SET name = ? WHERE id = ?")
            .bind(name.trim()).bind(&id).execute(pool).await?;
    }
    if let Some(color) = &patch.color {
        sqlx::query("UPDATE projects SET color = ? WHERE id = ?")
            .bind(color).bind(&id).execute(pool).await?;
    }
    if let Some(date) = &patch.target_date {
        let value = if date.is_empty() { None } else { Some(date.as_str()) };
        sqlx::query("UPDATE projects SET target_date = ? WHERE id = ?")
            .bind(value).bind(&id).execute(pool).await?;
    }
    if let Some(archived) = patch.archived {
        sqlx::query("UPDATE projects SET archived = ? WHERE id = ?")
            .bind(archived).bind(&id).execute(pool).await?;
    }
    // Изменение цели перезаряжает пуш о её выполнении в текущем периоде
    let mut goal_changed = false;
    if let Some(n) = patch.goal_tasks {
        sqlx::query("UPDATE projects SET goal_tasks = ? WHERE id = ?")
            .bind((n > 0).then_some(n)).bind(&id).execute(pool).await?;
        goal_changed = true;
    }
    if let Some(n) = patch.goal_mins {
        sqlx::query("UPDATE projects SET goal_mins = ? WHERE id = ?")
            .bind((n > 0).then_some(n)).bind(&id).execute(pool).await?;
        goal_changed = true;
    }
    if let Some(period) = &patch.goal_period {
        if period != "week" && period != "month" {
            return Err("Период цели: week или month".to_string().into());
        }
        sqlx::query("UPDATE projects SET goal_period = ? WHERE id = ?")
            .bind(period).bind(&id).execute(pool).await?;
        goal_changed = true;
    }
    if goal_changed {
        sqlx::query("UPDATE projects SET notified_goal = '' WHERE id = ?")
            .bind(&id).execute(pool).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_project(pool: State<'_, SqlitePool>, id: String) -> AppResult<()> {
    delete_project_impl(pool.inner(), id).await
}

pub async fn delete_project_impl(pool: &SqlitePool, id: String) -> AppResult<()> {
    // Задачи и заметки не удаляем — только отвязываем (FK не enforced)
    sqlx::query("UPDATE tasks SET project_id = NULL WHERE project_id = ?")
        .bind(&id).execute(pool).await?;
    sqlx::query("UPDATE notes SET project_id = NULL WHERE project_id = ?")
        .bind(&id).execute(pool).await?;
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(&id).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn create(pool: &SqlitePool, name: &str) -> Project {
        create_project_impl(pool, CreateProject {
            name: name.into(),
            color: "".into(),
            target_date: None,
        })
        .await
        .unwrap()
    }

    async fn insert_task(pool: &SqlitePool, project_id: Option<&str>, completed: bool) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden, created_at, updated_at, completed_at, project_id)
             VALUES (?, 'т', ?, 'Medium', 'Work', 'None', '[]', ?, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(if completed { "Done" } else { "Todo" })
            .bind(completed)
            .bind(completed.then(|| "2026-07-01T00:00:00+00:00"))
            .bind(project_id)
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn progress_counts_done_and_total() {
        let pool = test_pool().await;
        let p = create(&pool, "Ремонт").await;
        insert_task(&pool, Some(&p.id), false).await;
        insert_task(&pool, Some(&p.id), true).await; // выполненная (hidden) — в счётчике
        insert_task(&pool, None, true).await; // без проекта — не считается

        let projects = get_projects_impl(&pool).await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!((projects[0].task_done, projects[0].task_total), (1, 2));
    }

    #[tokio::test]
    async fn update_and_archive() {
        let pool = test_pool().await;
        let p = create(&pool, "Старое имя").await;

        update_project_impl(&pool, p.id.clone(), UpdateProject {
            name: Some("Новое имя".into()),
            color: Some("#ff0000".into()),
            target_date: Some("2026-12-31T00:00:00+00:00".into()),
            archived: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();

        let got = &get_projects_impl(&pool).await.unwrap()[0];
        assert_eq!(got.name, "Новое имя");
        assert_eq!(got.color, "#ff0000");
        assert_eq!(got.target_date.as_deref(), Some("2026-12-31T00:00:00+00:00"));
        assert!(got.archived);

        // пустая строка снимает дату
        update_project_impl(&pool, p.id.clone(), UpdateProject {
            target_date: Some(String::new()), ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(get_projects_impl(&pool).await.unwrap()[0].target_date, None);

        // пустое имя — ошибка
        assert!(update_project_impl(&pool, p.id, UpdateProject {
            name: Some("  ".into()), ..Default::default()
        })
        .await
        .is_err());
    }

    #[tokio::test]
    async fn goal_progress_counts_current_period_only() {
        let pool = test_pool().await;
        let p = create(&pool, "Учёба").await;
        let now = Utc::now();

        update_project_impl(&pool, p.id.clone(), UpdateProject {
            goal_tasks: Some(2), goal_mins: Some(120), goal_period: Some("week".into()),
            ..Default::default()
        })
        .await
        .unwrap();

        let insert = |completed_at: Option<String>, scheduled_at: Option<String>, mins: Option<i64>| {
            let pool = pool.clone();
            let pid = p.id.clone();
            async move {
                sqlx::query(
                    "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden,
                     created_at, updated_at, completed_at, project_id, scheduled_at, scheduled_mins)
                     VALUES (?, 'т', 'Todo', 'Medium', 'Work', 'None', '[]', 0, ?, ?, ?, ?, ?, ?)")
                    .bind(uuid::Uuid::new_v4().to_string())
                    .bind(now.to_rfc3339()).bind(now.to_rfc3339())
                    .bind(completed_at).bind(&pid).bind(scheduled_at).bind(mins)
                    .execute(&pool).await.unwrap();
            }
        };

        // выполнена только что — в периоде; выполнена 60 дней назад — нет
        insert(Some((now - chrono::Duration::minutes(1)).to_rfc3339()), None, None).await;
        insert(Some((now - chrono::Duration::days(60)).to_rfc3339()), None, None).await;
        // блок начался час назад — 45 мин в зачёт; блок в будущем — не считается
        insert(None, Some((now - chrono::Duration::hours(1)).to_rfc3339()), Some(45)).await;
        insert(None, Some((now + chrono::Duration::hours(3)).to_rfc3339()), Some(90)).await;

        let got = &get_projects_at(&pool, now).await.unwrap()[0];
        assert_eq!(got.goal_tasks, Some(2));
        assert_eq!(got.goal_mins, Some(120));
        assert_eq!(got.goal_done_tasks, 1);
        assert_eq!(got.goal_done_mins, 45);
        assert_eq!(got.task_total, 4);

        // goal_tasks: 0 снимает цель
        update_project_impl(&pool, p.id.clone(), UpdateProject {
            goal_tasks: Some(0), ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(get_projects_at(&pool, now).await.unwrap()[0].goal_tasks, None);
    }

    #[tokio::test]
    async fn delete_unlinks_tasks() {
        let pool = test_pool().await;
        let p = create(&pool, "Проект").await;
        insert_task(&pool, Some(&p.id), false).await;

        delete_project_impl(&pool, p.id).await.unwrap();

        assert!(get_projects_impl(&pool).await.unwrap().is_empty());
        let orphans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE project_id IS NOT NULL")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(orphans, 0);
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(total, 1); // задача жива, просто без проекта
    }
}
