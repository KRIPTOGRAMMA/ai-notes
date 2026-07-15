// ИИ-планировщик (v0.5 фаза 4): «Спланируй день» и «Что делать сейчас».
// Работает поверх данных фаз 1–3: бэклог, тайм-блоки, дедлайны, приоритеты.
//
// Паттерн как у остальных ИИ-команд: команда спавнит задачу, результат летит
// событием. Ответ модели для плана — строгий JSON, но парсим снисходительно
// (вырезаем [...]), а валидацию (id, окно, пересечения) делаем сами: модели
// доверять нельзя.

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use tauri::{Emitter, Manager};
use crate::commands::ai::ask_ai;

const SYSTEM_PLAN: &str = "Ты планировщик дня. Разложи самые важные задачи по свободному окну: \
близкие дедлайны и высокий приоритет — раньше, длительность блока 30–90 минут, \
между блоками перерыв 10–15 минут, занятые интервалы не пересекать. \
Планировать все задачи не обязательно — только что реально успеть. \
Ответь ТОЛЬКО JSON-массивом объектов вида {\"id\": \"...\", \"start\": \"HH:MM\", \"mins\": N}, без пояснений.";

const SYSTEM_WHAT_NOW: &str = "Ты коуч по продуктивности. По контексту посоветуй, чем заняться \
прямо сейчас, и почему — одним-двумя предложениями, по-русски, без списков и вступлений.";

const PLAN_END_HOUR: u32 = 22; // до скольки планируем день
const MAX_CANDIDATES: i64 = 15;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlannedBlock {
    pub id: String,
    pub title: String,
    pub scheduled_at: String, // RFC3339 — готово для update_task
    pub mins: i64,
}

#[derive(Clone, Serialize)]
pub struct PlanPayload {
    pub blocks: Vec<PlannedBlock>,
    pub error: Option<String>,
}

// Минуты от полуночи локального дня.
fn local_mins(dt: DateTime<Utc>) -> i64 {
    let l = dt.with_timezone(&chrono::Local);
    (l.hour() * 60 + l.minute()) as i64
}

fn fmt_hm(mins: i64) -> String {
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

pub struct PlanContext {
    pub prompt: String,
    pub candidates: HashMap<String, String>, // id -> title
    pub busy: Vec<(i64, i64)>,               // занятые интервалы, мин от полуночи
    pub window: (i64, i64),                  // свободное окно
}

// Контекст «Спланируй день»: свободное окно от «сейчас» (кратно 15 мин) до 22:00,
// занятость — сегодняшние блоки, кандидаты — бэклог по приоритету и дедлайну.
pub async fn plan_day_context(pool: &SqlitePool, now: DateTime<Utc>) -> Result<PlanContext, String> {
    let now_mins = local_mins(now);
    let window_start = ((now_mins + 14) / 15) * 15;
    let window_end = (PLAN_END_HOUR as i64) * 60;
    if window_start >= window_end {
        return Err(format!("День уже закончился (планирование до {}:00)", PLAN_END_HOUR));
    }

    // Занятость: блоки на сегодня (по локальной дате)
    let today = now.with_timezone(&chrono::Local).date_naive();
    let rows = sqlx::query(
        "SELECT title, scheduled_at, COALESCE(scheduled_mins, 60) AS mins
         FROM tasks WHERE hidden = 0 AND scheduled_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut busy: Vec<(i64, i64)> = vec![];
    let mut busy_lines: Vec<String> = vec![];
    for r in rows {
        let s: String = r.get("scheduled_at");
        let Ok(start) = DateTime::parse_from_rfc3339(&s) else { continue };
        let start = start.with_timezone(&Utc);
        if start.with_timezone(&chrono::Local).date_naive() != today { continue; }
        let from = local_mins(start);
        let mins: i64 = r.get("mins");
        busy.push((from, from + mins));
        busy_lines.push(format!("{}–{} {}", fmt_hm(from), fmt_hm(from + mins), r.get::<String, _>("title")));
    }
    busy.sort();

    // Кандидаты: бэклог без блока, важные и горящие — первыми
    let rows = sqlx::query(
        "SELECT id, title, priority, deadline FROM tasks
         WHERE hidden = 0 AND scheduled_at IS NULL AND status IN ('Todo', 'InProgress')
         ORDER BY CASE priority WHEN 'Critical' THEN 0 WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                  deadline IS NULL, deadline
         LIMIT ?",
    )
    .bind(MAX_CANDIDATES)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Err("Бэклог пуст — нечего планировать".into());
    }

    let mut candidates = HashMap::new();
    let mut task_lines = vec![];
    for r in rows {
        let id: String = r.get("id");
        let title: String = r.get("title");
        let deadline = r
            .get::<Option<String>, _>("deadline")
            .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
            .map(|d| d.with_timezone(&chrono::Local).format("%d.%m %H:%M").to_string())
            .unwrap_or_else(|| "нет".into());
        task_lines.push(format!(
            "{} | {} | приоритет {} | дедлайн {}",
            id, title, r.get::<String, _>("priority"), deadline
        ));
        candidates.insert(id, title);
    }

    let local = now.with_timezone(&chrono::Local);
    let prompt = format!(
        "Сегодня {}. Сейчас {}. Свободное окно: {}–{}.\nЗанято: {}.\nЗадачи (id | название | приоритет | дедлайн):\n{}",
        local.format("%d.%m.%Y"),
        fmt_hm(now_mins),
        fmt_hm(window_start),
        fmt_hm(window_end),
        if busy_lines.is_empty() { "ничего".into() } else { busy_lines.join("; ") },
        task_lines.join("\n"),
    );

    Ok(PlanContext { prompt, candidates, busy, window: (window_start, window_end) })
}

#[derive(Debug, Deserialize)]
struct RawPlanItem {
    id: String,
    start: String,
    mins: i64,
}

// Разбор и валидация ответа модели: неизвестные id, кривое время, выход за окно
// и пересечения (с занятым и друг с другом) молча выбрасываются.
pub fn parse_plan(raw: &str, ctx: &PlanContext) -> Vec<(String, i64, i64)> {
    let start_idx = raw.find('[');
    let end_idx = raw.rfind(']');
    let (Some(s), Some(e)) = (start_idx, end_idx) else { return vec![] };
    if s >= e { return vec![] }
    let Ok(items) = serde_json::from_str::<Vec<RawPlanItem>>(&raw[s..=e]) else { return vec![] };

    let mut taken = ctx.busy.clone();
    let mut plan: Vec<(String, i64, i64)> = vec![];
    let mut parsed: Vec<(String, i64, i64)> = items
        .into_iter()
        .filter_map(|it| {
            if !ctx.candidates.contains_key(&it.id) { return None; }
            let (h, m) = it.start.split_once(':')?;
            let start = h.trim().parse::<i64>().ok()? * 60 + m.trim().parse::<i64>().ok()?;
            let mins = (it.mins.clamp(15, 240) / 15) * 15;
            Some((it.id, start, mins))
        })
        .collect();
    parsed.sort_by_key(|&(_, start, _)| start);

    for (id, start, mins) in parsed {
        if start < ctx.window.0 || start + mins > ctx.window.1 { continue; }
        if taken.iter().any(|&(f, t)| start < t && start + mins > f) { continue; }
        if plan.iter().any(|p| p.0 == id) { continue; } // дубль задачи
        taken.push((start, start + mins));
        plan.push((id, start, mins));
    }
    plan
}

#[tauri::command]
pub async fn ai_plan_day(app: tauri::AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        let now = Utc::now();
        let r = async {
            let pool = app.state::<SqlitePool>();
            let ctx = plan_day_context(pool.inner(), now).await?;
            let raw = ask_ai(&app, SYSTEM_PLAN, &ctx.prompt).await?;
            let plan = parse_plan(&raw, &ctx);
            if plan.is_empty() {
                return Err(format!("Не удалось разобрать план модели: {}", raw.trim()));
            }
            let today = now.with_timezone(&chrono::Local).date_naive();
            Ok(plan
                .into_iter()
                .filter_map(|(id, start, mins)| {
                    let dt = today
                        .and_hms_opt((start / 60) as u32, (start % 60) as u32, 0)?
                        .and_local_timezone(chrono::Local)
                        .earliest()?
                        .with_timezone(&Utc);
                    Some(PlannedBlock {
                        title: ctx.candidates.get(&id).cloned().unwrap_or_default(),
                        id,
                        scheduled_at: dt.to_rfc3339(),
                        mins,
                    })
                })
                .collect::<Vec<_>>())
        }
        .await;
        let payload = match r {
            Ok(blocks) => PlanPayload { blocks, error: None },
            Err(e) => PlanPayload { blocks: vec![], error: Some(e) },
        };
        let _ = app.emit("ai-plan", payload);
    });
    Ok(())
}

// Контекст «Что делать сейчас»: текущий/следующий блок, просрочки, топ-приоритеты.
pub async fn what_now_context(pool: &SqlitePool, now: DateTime<Utc>) -> Result<String, String> {
    let mut lines = vec![format!("Сейчас {}.", fmt_hm(local_mins(now)))];

    let rows = sqlx::query(
        "SELECT title, scheduled_at, COALESCE(scheduled_mins, 60) AS mins
         FROM tasks WHERE hidden = 0 AND scheduled_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut current: Option<String> = None;
    let mut next: Option<(DateTime<Utc>, String)> = None;
    for r in rows {
        let s: String = r.get("scheduled_at");
        let Ok(start) = DateTime::parse_from_rfc3339(&s) else { continue };
        let start = start.with_timezone(&Utc);
        let mins: i64 = r.get("mins");
        let title: String = r.get("title");
        if start <= now && now < start + chrono::Duration::minutes(mins) {
            let end = local_mins(start) + mins;
            current = Some(format!("Идёт блок «{}» до {}.", title, fmt_hm(end)));
        } else if start > now && next.as_ref().is_none_or(|(n, _)| start < *n) {
            next = Some((start, title));
        }
    }
    if let Some(c) = current { lines.push(c); }
    if let Some((start, title)) = next {
        lines.push(format!("Следующий блок: {} «{}».", fmt_hm(local_mins(start)), title));
    }

    let overdue: Vec<String> = sqlx::query(
        "SELECT title FROM tasks WHERE hidden = 0 AND completed_at IS NULL
         AND deadline IS NOT NULL AND deadline <= ? ORDER BY deadline LIMIT 3",
    )
    .bind(now.to_rfc3339())
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .iter()
    .map(|r| r.get::<String, _>("title"))
    .collect();
    if !overdue.is_empty() {
        lines.push(format!("Просрочено: {}.", overdue.join(", ")));
    }

    let top: Vec<String> = sqlx::query(
        "SELECT title, priority FROM tasks
         WHERE hidden = 0 AND status IN ('Todo', 'InProgress')
         ORDER BY CASE priority WHEN 'Critical' THEN 0 WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END,
                  deadline IS NULL, deadline
         LIMIT 3",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .iter()
    .map(|r| format!("{} (приоритет {})", r.get::<String, _>("title"), r.get::<String, _>("priority")))
    .collect();
    if top.is_empty() {
        lines.push("Активных задач нет.".into());
    } else {
        lines.push(format!("Важные задачи: {}.", top.join("; ")));
    }

    Ok(lines.join(" "))
}

#[derive(Clone, Serialize)]
pub struct WhatNowPayload {
    pub result: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ai_what_now(app: tauri::AppHandle) -> Result<(), String> {
    tokio::spawn(async move {
        let r = async {
            let ctx = what_now_context(app.state::<SqlitePool>().inner(), Utc::now()).await?;
            ask_ai(&app, SYSTEM_WHAT_NOW, &ctx).await
        }
        .await;
        let (result, error) = match r {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        let _ = app.emit("ai-what-now", WhatNowPayload { result, error });
    });
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

    async fn insert_task(
        pool: &SqlitePool,
        title: &str,
        priority: &str,
        deadline: Option<String>,
        scheduled_at: Option<String>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, recurrence, tags, hidden,
             created_at, updated_at, deadline, scheduled_at, scheduled_mins)
             VALUES (?, ?, 'Todo', ?, 'Work', 'None', '[]', 0, ?, ?, ?, ?, 60)")
            .bind(&id).bind(title).bind(priority)
            .bind(&now).bind(&now)
            .bind(deadline).bind(scheduled_at)
            .execute(pool).await.unwrap();
        id
    }

    fn noon_utc() -> DateTime<Utc> {
        // Сегодняшний локальный полдень — окно 12:00–22:00 гарантированно открыто
        chrono::Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn plan_context_orders_and_marks_busy() {
        let pool = test_pool().await;
        let now = noon_utc();
        insert_task(&pool, "мелочь", "Low", None, None).await;
        insert_task(&pool, "горящая", "Critical", Some(now.to_rfc3339()), None).await;
        // блок сегодня в 14:00 — занятость
        let block_start = noon_utc() + chrono::Duration::hours(2);
        insert_task(&pool, "встреча", "Medium", None, Some(block_start.to_rfc3339())).await;

        let ctx = plan_day_context(&pool, now).await.unwrap();
        assert_eq!(ctx.candidates.len(), 2); // запланированная не кандидат
        assert_eq!(ctx.window.0, 12 * 60);
        assert_eq!(ctx.busy, vec![(14 * 60, 15 * 60)]);
        // критичная в промпте раньше мелочи
        let crit = ctx.prompt.find("горящая").unwrap();
        let low = ctx.prompt.find("мелочь").unwrap();
        assert!(crit < low, "{}", ctx.prompt);
        assert!(ctx.prompt.contains("14:00–15:00 встреча"), "{}", ctx.prompt);
    }

    #[tokio::test]
    async fn plan_context_errors_when_empty_or_late() {
        let pool = test_pool().await;
        assert!(plan_day_context(&pool, noon_utc()).await.is_err()); // пустой бэклог

        insert_task(&pool, "задача", "Medium", None, None).await;
        let late = noon_utc() + chrono::Duration::hours(10); // 22:00 — окно закрыто
        assert!(plan_day_context(&pool, late).await.is_err());
    }

    fn ctx_with(candidates: &[(&str, &str)], busy: Vec<(i64, i64)>) -> PlanContext {
        PlanContext {
            prompt: String::new(),
            candidates: candidates.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            busy,
            window: (9 * 60, 22 * 60),
        }
    }

    #[test]
    fn parse_plan_validates_everything() {
        let ctx = ctx_with(&[("a", "А"), ("b", "Б"), ("c", "В")], vec![(14 * 60, 15 * 60)]);
        let raw = r#"Вот план:
        [{"id":"a","start":"10:00","mins":50},
         {"id":"zzz","start":"11:00","mins":30},
         {"id":"b","start":"14:30","mins":30},
         {"id":"c","start":"08:00","mins":30},
         {"id":"a","start":"16:00","mins":30},
         {"id":"c","start":"10:20","mins":600}]"#;
        let plan = parse_plan(raw, &ctx);
        // a в 10:00 (mins снап к 45), zzz — чужой id, b пересекает занятое,
        // c в 08:00 — до окна, второй a — дубль, c в 10:20 — кламп 240 мин но пересекает a? нет: a 10:00–10:45, c 10:20 пересекает → выброшен
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0], ("a".to_string(), 10 * 60, 45));
    }

    #[test]
    fn parse_plan_accepts_clean_json_and_orders() {
        let ctx = ctx_with(&[("a", "А"), ("b", "Б")], vec![]);
        let raw = r#"[{"id":"b","start":"15:00","mins":60},{"id":"a","start":"09:00","mins":90}]"#;
        let plan = parse_plan(raw, &ctx);
        assert_eq!(plan, vec![("a".into(), 9 * 60, 90), ("b".into(), 15 * 60, 60)]);
    }

    #[test]
    fn parse_plan_garbage_is_empty() {
        let ctx = ctx_with(&[("a", "А")], vec![]);
        assert!(parse_plan("не могу помочь", &ctx).is_empty());
        assert!(parse_plan("[]", &ctx).is_empty());
        assert!(parse_plan("[{\"id\":1}]", &ctx).is_empty());
    }

    #[tokio::test]
    async fn what_now_context_mentions_block_overdue_and_priorities() {
        let pool = test_pool().await;
        let now = noon_utc();
        // идущий блок: начался в 11:30 на 60 мин
        insert_task(&pool, "фокус", "Medium", None,
            Some((now - chrono::Duration::minutes(30)).to_rfc3339())).await;
        // следующий блок в 15:00
        insert_task(&pool, "созвон", "Medium", None,
            Some((now + chrono::Duration::hours(3)).to_rfc3339())).await;
        insert_task(&pool, "просроченная", "High",
            Some((now - chrono::Duration::hours(2)).to_rfc3339()), None).await;

        let s = what_now_context(&pool, now).await.unwrap();
        assert!(s.contains("Идёт блок «фокус» до 12:30"), "{s}");
        assert!(s.contains("Следующий блок: 15:00 «созвон»"), "{s}");
        assert!(s.contains("Просрочено: просроченная"), "{s}");
        assert!(s.contains("Важные задачи:"), "{s}");
    }
}
