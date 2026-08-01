-- The Notification Centre: a log of every push sent inside the application, so a
-- missed system notification is not lost for good. kind is a stable tag for the
-- source (deadline/block/digest/goal/app_limit/pomodoro/overdue/missed_days/
-- nudge/activity_return), used for the icon and filtering in the feed.
CREATE TABLE IF NOT EXISTS notification_log (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    read_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_notification_log_created_at ON notification_log (created_at DESC);
