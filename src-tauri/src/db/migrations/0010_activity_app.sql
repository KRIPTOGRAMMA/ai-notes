-- Per-application tracking. Every Active row of the log records the class of the
-- focused window at the moment of the tick (NULL when no window provider is
-- available, or when idle).
ALTER TABLE activity_log ADD COLUMN app TEXT;
