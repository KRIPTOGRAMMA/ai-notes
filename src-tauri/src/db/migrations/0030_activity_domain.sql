-- The domain of the active browser tab. Always NULL until the user enables
-- track_domains in Settings (off by default). Only the domain is stored — the
-- window title never reaches the DB.
ALTER TABLE activity_log ADD COLUMN domain TEXT;
