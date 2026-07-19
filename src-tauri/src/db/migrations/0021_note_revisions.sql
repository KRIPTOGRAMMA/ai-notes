CREATE TABLE IF NOT EXISTS note_revisions (
    id TEXT PRIMARY KEY,
    note_id TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_note_revisions_note_id ON note_revisions(note_id);
