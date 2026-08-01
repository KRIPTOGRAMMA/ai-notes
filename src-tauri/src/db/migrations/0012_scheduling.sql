-- Time blocking. A block is planned working time on a task, independent of the
-- deadline. notified_block records that the "block started" push has been sent
-- (reset when the block moves, like the deadline's notified_* flags).
ALTER TABLE tasks ADD COLUMN scheduled_at TEXT;
ALTER TABLE tasks ADD COLUMN scheduled_mins INTEGER;
ALTER TABLE tasks ADD COLUMN notified_block INTEGER NOT NULL DEFAULT 0;
