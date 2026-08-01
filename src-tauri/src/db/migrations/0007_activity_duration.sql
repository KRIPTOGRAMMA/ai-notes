-- Every activity_log row now carries its own duration: the tick interval is
-- configurable, so "COUNT(*) * a constant" no longer works. Legacy rows were
-- written with a 60-second tick, hence DEFAULT 60.
ALTER TABLE activity_log ADD COLUMN duration_secs INTEGER NOT NULL DEFAULT 60;
