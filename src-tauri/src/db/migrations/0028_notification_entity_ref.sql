-- Клик по уведомлению в Центре уведомлений → открыть связанную сущность
-- (v0.9.18, изначально для напоминаний заметок). NULL/NULL для уведомлений
-- без цели перехода (дедлайны и т.п. пока не размечены — можно добавить позже).
ALTER TABLE notification_log ADD COLUMN entity_type TEXT;
ALTER TABLE notification_log ADD COLUMN entity_id TEXT;
