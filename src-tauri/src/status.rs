// CLI-режим `ai-notes --status`: короткоживущий процесс для waybar custom
// module (и любого другого статус-бара). Открывает БД read-only (WAL позволяет
// читать параллельно с работающим приложением), печатает одну строку JSON в
// stdout и выходит — Tauri не поднимается, single-instance не задевается.
//
// Приоритет текста: активная сессия трекинга → идущий тайм-блок → идущая
// рутина → следующий блок → следующая рутина → задача InProgress → счётчик
// задач с дедлайном на сегодня → «свободно». Режим работы и пауза уведомлений —
// в tooltip. Помодоро-таймер — рантайм-состояние приложения, в БД его нет,
// поэтому в статусе не показывается.

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

const TITLE_MAX: usize = 28;

#[derive(Debug, Serialize, PartialEq)]
pub struct StatusPayload {
    pub text: String,
    pub tooltip: String,
    // Для стилизации в waybar: tracking | block | next | task | due | idle | off
    pub class: String,
    // Режим работы (Light | Study | Focus) — для format-icons
    pub alt: String,
}

fn empty_payload() -> StatusPayload {
    StatusPayload {
        text: String::new(),
        tooltip: "AI Notes: БД не найдена".into(),
        class: "off".into(),
        alt: String::new(),
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

fn hhmm(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%H:%M").to_string()
}

struct Block {
    title: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

pub async fn status_payload(pool: &SqlitePool, now: DateTime<Utc>) -> Result<StatusPayload, sqlx::Error> {
    // Тайм-блоки сегодняшнего локального дня (не Done, не скрытые)
    let rows = sqlx::query(
        "SELECT title, scheduled_at, COALESCE(scheduled_mins, 60) AS mins FROM tasks
         WHERE hidden = 0 AND status != 'Done' AND scheduled_at IS NOT NULL AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let today = now.with_timezone(&Local).date_naive();
    let mut blocks: Vec<Block> = rows
        .into_iter()
        .filter_map(|r| {
            let start = DateTime::parse_from_rfc3339(&r.get::<String, _>("scheduled_at"))
                .ok()?
                .with_timezone(&Utc);
            if start.with_timezone(&Local).date_naive() != today {
                return None;
            }
            Some(Block {
                title: r.get("title"),
                start,
                end: start + Duration::minutes(r.get::<i64, _>("mins")),
            })
        })
        .collect();
    blocks.sort_by_key(|b| b.start);

    let current = blocks.iter().filter(|b| b.start <= now && now < b.end).last();
    let next = blocks.iter().find(|b| b.start > now);

    // Рутины на сегодня: время в минутах от полуночи → абсолютное время сегодня
    let routine_rows = sqlx::query(
        "SELECT title, start_mins, duration_mins FROM routines
         WHERE active = 1 AND (days_mask & ?) != 0
         ORDER BY start_mins"
    )
    .bind(1i64 << today.weekday().num_days_from_monday())
    .fetch_all(pool)
    .await?;

    let mut routine_blocks: Vec<Block> = routine_rows.iter().filter_map(|r| {
        let start_mins: i64 = r.get("start_mins");
        let dur: i64 = r.get("duration_mins");
        let start = Local
            .from_local_datetime(&today.and_time(NaiveTime::from_hms_opt(
                (start_mins / 60) as u32,
                (start_mins % 60) as u32,
                0,
            )?))
            .single()?
            .with_timezone(&Utc);
        Some(Block {
            title: r.get("title"),
            start,
            end: start + Duration::minutes(dur),
        })
    }).collect();
    routine_blocks.sort_by_key(|b| b.start);
    let routine_current = routine_blocks.iter().filter(|b| b.start <= now && now < b.end).last();
    let routine_next = routine_blocks.iter().find(|b| b.start > now);

    let in_progress: Option<String> = sqlx::query(
        "SELECT title FROM tasks WHERE hidden = 0 AND status = 'InProgress' AND deleted_at IS NULL
         ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.get("title"));

    // Дедлайн «на сегодня» = до локальной полуночи, просрочка входит.
    // Сравнение строк корректно: оба операнда — RFC3339 в UTC.
    let tomorrow_local = today.succ_opt().unwrap_or(today);
    let tomorrow_utc = Local
        .from_local_datetime(&tomorrow_local.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now);
    let due_row = sqlx::query(
        "SELECT COUNT(*) AS due,
                SUM(CASE WHEN deadline < ? THEN 1 ELSE 0 END) AS overdue
         FROM tasks
         WHERE hidden = 0 AND status != 'Done' AND deadline IS NOT NULL AND deadline < ? AND deleted_at IS NULL",
    )
    .bind(now.to_rfc3339())
    .bind(tomorrow_utc.to_rfc3339())
    .fetch_one(pool)
    .await?;
    let due: i64 = due_row.get("due");
    let overdue: i64 = due_row.get::<Option<i64>, _>("overdue").unwrap_or(0);

    // Активная сессия трекинга (v0.7.9)
    let active_session: Option<(String, i64)> = sqlx::query(
        "SELECT s.started_at, t.title
         FROM task_sessions s
         JOIN tasks t ON t.id = s.task_id
         WHERE s.ended_at IS NULL
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await?
    .and_then(|r| {
        let started: String = r.get("started_at");
        let started_dt = DateTime::parse_from_rfc3339(&started).ok()?.with_timezone(&Utc);
        let mins = (now - started_dt).num_seconds().max(0) / 60;
        Some((r.get::<String, _>("title"), mins))
    });

    let setting = |key: &str| {
        let pool = pool.clone();
        let key = key.to_string();
        async move {
            sqlx::query("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .map(|r| r.get::<String, _>("value"))
        }
    };
    let work_mode = setting("work_mode").await.unwrap_or_else(|| "Light".into());
    let quiet_until = setting("quiet_until").await.unwrap_or_default();
    let pomo_phase = setting("pomodoro_phase").await.unwrap_or_else(|| "off".into());
    let pomo_until = setting("pomodoro_until")
        .await
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc));

    // Помодоро — самое сиюминутное состояние (v0.6.6: раньше не показывалось,
    // т.к. фаза жила только в рантайме и короткоживущий CLI её не видел).
    let pomo_label = match (pomo_phase.as_str(), pomo_until) {
        ("work", Some(t)) if t > now => Some(format!("🍅 до {}", hhmm(t))),
        ("break", Some(t)) if t > now => Some(format!("☕ до {}", hhmm(t))),
        ("paused", _) => Some("🍅 пауза".to_string()),
        _ => None,
    };

    let (text, class) = if let Some(label) = &pomo_label {
        (label.clone(), "pomodoro")
    } else if let Some((ref title, mins)) = active_session {
        (format!("▶ {} · {} мин", ellipsize(title, TITLE_MAX), mins), "tracking")
    } else if let Some(b) = current {
        (format!("▶ {} до {}", ellipsize(&b.title, TITLE_MAX), hhmm(b.end)), "block")
    } else if let Some(b) = routine_current {
        (format!("▶ {} до {}", ellipsize(&b.title, TITLE_MAX), hhmm(b.end)), "block")
    } else if let Some(b) = next {
        (format!("⏱ {} {}", hhmm(b.start), ellipsize(&b.title, TITLE_MAX)), "next")
    } else if let Some(b) = routine_next {
        (format!("⏱ {} {}", hhmm(b.start), ellipsize(&b.title, TITLE_MAX)), "next")
    } else if let Some(t) = &in_progress {
        (format!("▶ {}", ellipsize(t, TITLE_MAX)), "task")
    } else if due > 0 {
        (format!("☑ {due}"), "due")
    } else {
        ("✓".into(), "idle")
    };

    let mut tip: Vec<String> = Vec::new();
    if let Some((ref title, mins)) = active_session {
        tip.push(format!("Трекинг: {} ({} мин)", title, mins));
    }
    if pomo_label.is_some() {
        let phase_ru = match pomo_phase.as_str() {
            "work" => "работа",
            "break" => "перерыв",
            "paused" => "на паузе",
            _ => "",
        };
        tip.push(format!("Помодоро: {phase_ru}"));
    }
    if let Some(b) = current {
        tip.push(format!("Идёт: {} (до {})", b.title, hhmm(b.end)));
    } else if let Some(b) = routine_current {
        tip.push(format!("Идёт рутина: {} (до {})", b.title, hhmm(b.end)));
    }
    if let Some(b) = next {
        tip.push(format!("Далее: {} в {}", b.title, hhmm(b.start)));
    } else if let Some(b) = routine_next {
        tip.push(format!("Далее рутина: {} в {}", b.title, hhmm(b.start)));
    }
    if let Some(t) = &in_progress {
        tip.push(format!("В работе: {t}"));
    }
    if due > 0 {
        let mut line = format!("Задач на сегодня: {due}");
        if overdue > 0 {
            line.push_str(&format!(" (просрочено: {overdue})"));
        }
        tip.push(line);
    }
    tip.push(format!("Режим: {work_mode}"));
    if quiet_until == crate::commands::settings::QUIET_FOREVER {
        tip.push("Уведомления: выключены".into());
    } else if let Ok(t) = DateTime::parse_from_rfc3339(&quiet_until) {
        if now < t.with_timezone(&Utc) {
            tip.push(format!("Уведомления: пауза до {}", hhmm(t.with_timezone(&Utc))));
        }
    }

    Ok(StatusPayload {
        text,
        tooltip: tip.join("\n"),
        class: class.into(),
        alt: work_mode,
    })
}

async fn open_readonly() -> Option<SqlitePool> {
    // Тот же путь, что app.path().app_data_dir() у Tauri: data_dir + identifier
    // (см. tauri.conf.json). mode=ro — файл не создаём и не трогаем схему.
    let path = dirs::data_dir()?.join("com.ainotes.app").join("data.db");
    if !path.exists() {
        return None;
    }
    SqlitePool::connect(&format!("sqlite:{}?mode=ro", path.display()))
        .await
        .ok()
}

// Точка входа CLI: печатает JSON для waybar и возвращается (вызывающий выходит).
pub fn print_status() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let payload = rt.block_on(async {
        match open_readonly().await {
            Some(pool) => status_payload(&pool, Utc::now()).await.unwrap_or_else(|_| empty_payload()),
            None => empty_payload(),
        }
    });
    println!("{}", serde_json::to_string(&payload).expect("status json"));
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    // Сегодня, локальный полдень — детерминированное «сейчас» без краёв суток.
    fn noon_utc() -> DateTime<Utc> {
        let today = Local::now().date_naive();
        Local
            .from_local_datetime(&today.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc)
    }

    async fn insert_task(
        pool: &SqlitePool,
        title: &str,
        status: &str,
        deadline: Option<DateTime<Utc>>,
        scheduled_at: Option<DateTime<Utc>>,
        mins: Option<i64>,
    ) {
        sqlx::query(
            "INSERT INTO tasks (id, title, status, deadline, scheduled_at, scheduled_mins, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(title)
        .bind(status)
        .bind(deadline.map(|t| t.to_rfc3339()))
        .bind(scheduled_at.map(|t| t.to_rfc3339()))
        .bind(mins)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn pomodoro_takes_priority_over_everything() {
        let pool = test_pool().await;
        let now = noon_utc();
        // Идущий блок есть, но помодоро всё равно должно выиграть приоритет
        insert_task(&pool, "писать отчёт", "Todo", None, Some(now - Duration::minutes(30)), Some(60)).await;

        for (k, v) in [
            ("pomodoro_phase", "work".to_string()),
            ("pomodoro_until", (now + Duration::minutes(12)).to_rfc3339()),
        ] {
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(k).bind(v).execute(&pool).await.unwrap();
        }
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "pomodoro");
        assert!(p.text.starts_with("🍅 до "), "text: {}", p.text);
        assert!(p.tooltip.contains("Помодоро: работа"));

        // Перерыв — другой символ и метка
        sqlx::query("UPDATE settings SET value = 'break' WHERE key = 'pomodoro_phase'")
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert!(p.text.starts_with("☕ до "), "text: {}", p.text);
        assert!(p.tooltip.contains("Помодоро: перерыв"));

        // Пауза — без времени
        sqlx::query("UPDATE settings SET value = 'paused' WHERE key = 'pomodoro_phase'")
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.text, "🍅 пауза");

        // Истёкшая фаза (цикл ещё не тикнул) не должна перекрывать блок
        sqlx::query("UPDATE settings SET value = 'work' WHERE key = 'pomodoro_phase'")
            .execute(&pool).await.unwrap();
        sqlx::query("UPDATE settings SET value = ? WHERE key = 'pomodoro_until'")
            .bind((now - Duration::minutes(1)).to_rfc3339())
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "block");

        // "off" — не показывается вовсе
        sqlx::query("UPDATE settings SET value = 'off' WHERE key = 'pomodoro_phase'")
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "block");
    }

    #[tokio::test]
    async fn empty_db_is_idle() {
        let pool = test_pool().await;
        let p = status_payload(&pool, noon_utc()).await.unwrap();
        assert_eq!(p.text, "✓");
        assert_eq!(p.class, "idle");
        assert_eq!(p.alt, "Light"); // дефолтный режим без настроек
        assert!(p.tooltip.contains("Режим: Light"));
    }

    #[tokio::test]
    async fn current_block_wins_and_shows_end_time() {
        let pool = test_pool().await;
        let now = noon_utc();
        // Идущий блок 11:30–12:30, следующий в 14:00, плюс InProgress-задача
        insert_task(&pool, "писать отчёт", "Todo", None, Some(now - Duration::minutes(30)), Some(60)).await;
        insert_task(&pool, "созвон", "Todo", None, Some(now + Duration::hours(2)), Some(30)).await;
        insert_task(&pool, "фоновая задача", "InProgress", None, None, None).await;

        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "block");
        assert!(p.text.starts_with("▶ писать отчёт до "), "text: {}", p.text);
        assert!(p.text.ends_with(&hhmm(now + Duration::minutes(30))));
        assert!(p.tooltip.contains("Далее: созвон"));
        assert!(p.tooltip.contains("В работе: фоновая задача"));
    }

    #[tokio::test]
    async fn next_block_then_inprogress_then_due() {
        let pool = test_pool().await;
        let now = noon_utc();

        // Только дедлайны: один просрочен, один вечером
        insert_task(&pool, "просроченная", "Todo", Some(now - Duration::hours(3)), None, None).await;
        insert_task(&pool, "вечерняя", "Todo", Some(now + Duration::hours(5)), None, None).await;
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.text, "☑ 2");
        assert_eq!(p.class, "due");
        assert!(p.tooltip.contains("Задач на сегодня: 2 (просрочено: 1)"));

        // Появилась InProgress — приоритетнее счётчика
        insert_task(&pool, "важное дело прямо сейчас", "InProgress", None, None, None).await;
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "task");
        assert!(p.text.starts_with("▶ важное дело"));

        // Будущий блок сегодня — приоритетнее InProgress
        insert_task(&pool, "блок после обеда", "Todo", None, Some(now + Duration::hours(1)), Some(45)).await;
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "next");
        assert!(p.text.contains("блок после обеда"));

        // Завершённые и вчерашние блоки не считаются
        insert_task(&pool, "вчерашний блок", "Todo", None, Some(now - Duration::days(1)), Some(60)).await;
        insert_task(&pool, "сделанный блок", "Done", None, Some(now - Duration::minutes(10)), Some(60)).await;
        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.class, "next", "Done/вчерашние блоки не должны влиять");
    }

    #[tokio::test]
    async fn mode_and_quiet_pause_in_tooltip() {
        let pool = test_pool().await;
        let now = noon_utc();
        for (k, v) in [
            ("work_mode", "Focus".to_string()),
            ("quiet_until", (now + Duration::minutes(45)).to_rfc3339()),
        ] {
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(k).bind(v).execute(&pool).await.unwrap();
        }

        let p = status_payload(&pool, now).await.unwrap();
        assert_eq!(p.alt, "Focus");
        assert!(p.tooltip.contains("Режим: Focus"));
        assert!(p.tooltip.contains("Уведомления: пауза до"));

        // Истёкшая пауза не показывается
        sqlx::query("UPDATE settings SET value = ? WHERE key = 'quiet_until'")
            .bind((now - Duration::minutes(1)).to_rfc3339())
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert!(!p.tooltip.contains("Уведомления"));

        // Бессрочная пауза
        sqlx::query("UPDATE settings SET value = ? WHERE key = 'quiet_until'")
            .bind(crate::commands::settings::QUIET_FOREVER)
            .execute(&pool).await.unwrap();
        let p = status_payload(&pool, now).await.unwrap();
        assert!(p.tooltip.contains("Уведомления: выключены"));
    }

    #[test]
    fn ellipsize_respects_chars_not_bytes() {
        assert_eq!(ellipsize("короткое", 28), "короткое");
        let long = "очень длинное название задачи которое не влезает";
        let cut = ellipsize(long, 10);
        assert_eq!(cut.chars().count(), 11); // 10 символов + …
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn payload_serializes_to_waybar_json() {
        let p = StatusPayload {
            text: "▶ задача до 13:00".into(),
            tooltip: "Режим: Light".into(),
            class: "block".into(),
            alt: "Light".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"text\":"));
        assert!(json.contains("\"tooltip\":"));
        assert!(json.contains("\"class\":\"block\""));
    }
}
