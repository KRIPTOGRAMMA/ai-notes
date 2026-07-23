-- Напоминание у заметки (v0.9.18): reminder_at — RFC3339, NULL = без напоминания.
-- notified_reminder — тот же dedup-паттерн, что notified_block/notified_24h у
-- задач: сбрасывается в 0, когда reminder_at меняется, чтобы пуш пришёл заново.
ALTER TABLE notes ADD COLUMN reminder_at TEXT;
ALTER TABLE notes ADD COLUMN notified_reminder INTEGER NOT NULL DEFAULT 0;
