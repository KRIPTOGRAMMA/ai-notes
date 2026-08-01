// Buttons on a deadline notification: "Done" and "Snooze for an hour", without
// opening the application.
//
// tauri-plugin-notification accepts actions in its builder but ignores them
// entirely on desktop: verified against the plugin's own code (2.3.3,
// desktop.rs::show() passes only title/body/icon/sound into notify_rust, never
// calls .action() and never subscribes to the D-Bus action signal). The lower
// layer supports the freedesktop protocol, so the buttons are only reachable by
// calling notify_rust directly.
//
// That makes this the first code in the project bypassing an official Tauri plugin
// rather than extending it, and the trade-off is deliberate: the buttons exist on
// Linux and nowhere else. Every other platform keeps the plugin path unchanged (see
// send_notification_for), so nothing regresses there — it is the same capability
// detection the project already uses for window tracking and ext-idle-notify.
//
// Waiting for a click is blocking (notify_rust::Notification::show() returns a
// handle whose wait_for_action consumes the current thread), so each notification
// with actions gets its own detached thread. The thread ends when the user clicks
// or the notification expires.

/// What the user pressed on a notification.
pub enum DeadlineAction {
    Done,
    SnoozeHour,
}

/// Applies the action to a task. Kept separate from the D-Bus plumbing so the
/// behaviour can be unit-tested: the plumbing needs a live session bus, this does
/// not.
///
/// "Snooze" shifts the deadline by an hour from now rather than from the old
/// deadline: the point is "remind me in an hour", and for an already-overdue task
/// shifting from the old value could land in the past again and fire immediately.
///
/// The notified_* flags are reset because they refer to the previous deadline —
/// the same reasoning as update_task, where without a reset the scheduler would
/// never notify about the new date.
pub async fn apply_deadline_action(
    pool: &sqlx::SqlitePool,
    task_id: &str,
    action: DeadlineAction,
) -> Result<(), String> {
    match action {
        DeadlineAction::Done => {
            crate::commands::tasks::complete_task_impl(pool, task_id.to_string())
                .await
                .map(|_| ())
        }
        DeadlineAction::SnoozeHour => {
            let next = chrono::Utc::now() + chrono::Duration::hours(1);
            sqlx::query(
                "UPDATE tasks
                 SET deadline = ?, notified_24h = 0, notified_1h = 0, notified_deadline = 0
                 WHERE id = ?",
            )
            .bind(next.to_rfc3339())
            .bind(task_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

/// Maps the freedesktop action id back to an action. Unknown ids yield None:
/// the notification daemon may report "default" (the body was clicked) or
/// "__closed", and neither must be mistaken for a button press.
pub fn action_from_id(id: &str) -> Option<DeadlineAction> {
    match id {
        "done" => Some(DeadlineAction::Done),
        "snooze" => Some(DeadlineAction::SnoozeHour),
        _ => None,
    }
}

/// Shows a deadline notification carrying buttons and waits for a click in a
/// separate thread.
///
/// Returns false when the notification could not be shown (no D-Bus session, a
/// daemon that rejects actions), so the caller can fall back to the plugin instead
/// of leaving the user with no notification at all.
#[cfg(target_os = "linux")]
pub fn show_with_actions(
    app: tauri::AppHandle,
    pool: sqlx::SqlitePool,
    task_id: String,
    title: &str,
    body: &str,
    done_label: &str,
    snooze_label: &str,
) -> bool {
    use tauri::Emitter;

    let handle = match notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .action("done", done_label)
        .action("snooze", snooze_label)
        .show()
    {
        Ok(h) => h,
        Err(_) => return false,
    };

    std::thread::spawn(move || {
        handle.wait_for_action(|id| {
            let Some(action) = action_from_id(id) else { return };
            // The callback runs on the D-Bus thread with no async runtime, so the
            // work is handed to the app's runtime rather than blocking here.
            tauri::async_runtime::spawn(async move {
                let _ = apply_deadline_action(&pool, &task_id, action).await;
                // The lists are refreshed by the same event the quick-capture window
                // uses: the section may not even be mounted, while the store is shared.
                let _ = app.emit("task-created", ());
            });
        });
    });

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    async fn task_with_deadline(pool: &sqlx::SqlitePool, deadline: chrono::DateTime<chrono::Utc>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tasks (id, title, status, priority, category, deadline, tags, recurrence,
                                hidden, created_at, updated_at, sort_order,
                                notified_24h, notified_1h, notified_deadline)
             VALUES (?, 'дедлайн', 'Todo', 'Medium', 'Work', ?, '[]', 'None', 0, ?, ?, 1, 1, 1, 1)",
        )
        .bind(&id)
        .bind(deadline.to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    // Only the two real buttons count. A daemon also reports "default" (the body was
    // clicked) and "__closed" — treating either as a press would complete a task the
    // user merely dismissed.
    #[test]
    fn only_known_action_ids_are_recognized() {
        assert!(matches!(action_from_id("done"), Some(DeadlineAction::Done)));
        assert!(matches!(action_from_id("snooze"), Some(DeadlineAction::SnoozeHour)));
        for id in ["default", "__closed", "", "Done", "снуз"] {
            assert!(action_from_id(id).is_none(), "id {id:?} must not be an action");
        }
    }

    #[tokio::test]
    async fn done_completes_the_task() {
        let pool = test_pool().await;
        let id = task_with_deadline(&pool, chrono::Utc::now()).await;

        apply_deadline_action(&pool, &id, DeadlineAction::Done).await.unwrap();

        let (status, hidden): (String, bool) =
            sqlx::query_as("SELECT status, hidden FROM tasks WHERE id = ?")
                .bind(&id).fetch_one(&pool).await.unwrap();
        assert_eq!(status, "Done");
        assert!(hidden, "a completed task leaves the active list");
    }

    // Snoozing shifts from NOW rather than from the old deadline: for an overdue task
    // shifting from the old value would land in the past again and fire immediately.
    #[tokio::test]
    async fn snooze_shifts_an_hour_from_now_even_when_overdue() {
        let pool = test_pool().await;
        let long_ago = chrono::Utc::now() - chrono::Duration::days(3);
        let id = task_with_deadline(&pool, long_ago).await;

        apply_deadline_action(&pool, &id, DeadlineAction::SnoozeHour).await.unwrap();

        let deadline: String = sqlx::query_scalar("SELECT deadline FROM tasks WHERE id = ?")
            .bind(&id).fetch_one(&pool).await.unwrap();
        let parsed = chrono::DateTime::parse_from_rfc3339(&deadline).unwrap().with_timezone(&chrono::Utc);
        let delta = (parsed - chrono::Utc::now()).num_minutes();
        assert!((55..=65).contains(&delta), "expected about an hour ahead, got {delta} minutes");
    }

    // The flags refer to the previous deadline. Without a reset the scheduler would
    // never notify about the new one — the very bug update_task had to fix.
    #[tokio::test]
    async fn snooze_rearms_the_notifications() {
        let pool = test_pool().await;
        let id = task_with_deadline(&pool, chrono::Utc::now()).await;

        apply_deadline_action(&pool, &id, DeadlineAction::SnoozeHour).await.unwrap();

        let (h24, h1, at): (bool, bool, bool) = sqlx::query_as(
            "SELECT notified_24h, notified_1h, notified_deadline FROM tasks WHERE id = ?",
        )
        .bind(&id).fetch_one(&pool).await.unwrap();
        assert!(!h24 && !h1 && !at, "every flag must be reset");
    }

    // A blocked task cannot be completed, and the button must not become a way around
    // that: the ban lives in complete_task_impl precisely because a task can also be
    // closed from the tray, the quick slot and the palette.
    #[tokio::test]
    async fn done_respects_the_dependency_ban() {
        let pool = test_pool().await;
        let blocker = task_with_deadline(&pool, chrono::Utc::now()).await;
        let blocked = task_with_deadline(&pool, chrono::Utc::now()).await;
        crate::commands::dependencies::add_task_dependency_impl(&pool, &blocked, &blocker)
            .await
            .unwrap();

        assert!(apply_deadline_action(&pool, &blocked, DeadlineAction::Done).await.is_err());

        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = ?")
            .bind(&blocked).fetch_one(&pool).await.unwrap();
        assert_eq!(status, "Todo", "the blocked task stays open");
    }
}
