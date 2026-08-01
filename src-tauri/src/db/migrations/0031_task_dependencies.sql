-- Task dependencies: "B is blocked by A".
--
-- A separate table rather than a blocked_by column on tasks: a task may have
-- several blockers and is only freed once all of them are closed. It costs the
-- same as a single column but will not become a dead end later.
--
-- ON DELETE CASCADE concerns the physical removal of a task row. The Trash does
-- not use it: the Trash is soft (tasks.deleted_at), so a dependency survives a
-- blocker being trashed and returns with it on restore. Treating a trashed
-- blocker as non-blocking is handled by the SQL in commands/dependencies.rs, not
-- by this schema.
--
-- PRIMARY KEY (task_id, blocker_id) suppresses duplicate links by itself.
CREATE TABLE task_dependencies (
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    blocker_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (task_id, blocker_id)
);

-- We traverse in both directions: "who blocks me" when rendering the task list,
-- and "whom does this unblock" when it is completed.
CREATE INDEX idx_task_deps_task ON task_dependencies(task_id);
CREATE INDEX idx_task_deps_blocker ON task_dependencies(blocker_id);
