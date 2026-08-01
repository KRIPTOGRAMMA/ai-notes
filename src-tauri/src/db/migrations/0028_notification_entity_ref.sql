-- Clicking a notification in the Notification Centre opens the linked entity
-- (introduced for note reminders). NULL/NULL for notifications with no navigation
-- target (deadlines and the like are not annotated yet; that can be added later).
ALTER TABLE notification_log ADD COLUMN entity_type TEXT;
ALTER TABLE notification_log ADD COLUMN entity_id TEXT;
