-- v0.5 фаза 3: тайм-блокинг. Блок = запланированное время работы над задачей,
-- независим от дедлайна. notified_block — пуш «блок начался» уже отправлен
-- (сбрасывается при переносе блока, как notified_* у дедлайна).
ALTER TABLE tasks ADD COLUMN scheduled_at TEXT;
ALTER TABLE tasks ADD COLUMN scheduled_mins INTEGER;
ALTER TABLE tasks ADD COLUMN notified_block INTEGER NOT NULL DEFAULT 0;
