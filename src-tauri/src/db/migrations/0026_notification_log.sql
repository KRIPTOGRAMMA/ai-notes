-- Центр уведомлений (v0.9.16): лог всех отправленных пушей внутри приложения,
-- чтобы пропущенное системное уведомление не терялось безвозвратно.
-- kind — стабильный тег источника (deadline/block/digest/goal/app_limit/
-- pomodoro/overdue/missed_days/nudge/activity_return), для иконки/фильтра в ленте.
CREATE TABLE IF NOT EXISTS notification_log (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    read_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_notification_log_created_at ON notification_log (created_at DESC);
