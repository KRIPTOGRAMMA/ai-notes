-- A note's reminder: reminder_at is RFC3339, NULL means no reminder.
-- notified_reminder uses the same dedup pattern as notified_block/notified_24h on
-- tasks: reset to 0 whenever reminder_at changes, so the push fires again.
ALTER TABLE notes ADD COLUMN reminder_at TEXT;
ALTER TABLE notes ADD COLUMN notified_reminder INTEGER NOT NULL DEFAULT 0;
