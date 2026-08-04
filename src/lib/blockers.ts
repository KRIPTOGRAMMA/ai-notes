// The reverse side of task dependencies (v0.9.78).
//
// The backend answers only one direction: task.blocked_by lists what holds this
// task up (attach_blockers, filled on every get_tasks). The other direction —
// "what does finishing this unblock?" — is what tells you which task to take
// first to clear a jam, and it is a pure inversion of an array already in memory.
// No backend query is needed, so none was added.

import type { Task } from "./types";

/**
 * Maps a blocker's id to the number of tasks it is currently holding up.
 *
 * Only ids that block something appear, so a lookup miss means "blocks nothing" —
 * the caller does not have to distinguish an absent key from a zero.
 *
 * Counting is restricted to the tasks passed in, on purpose: `blocked_by` already
 * excludes blockers that are done, hidden or in the Trash (OPEN_BLOCKER in
 * dependencies.rs), but nothing stops a *blocked* task from being completed
 * itself. Passing the visible active list keeps the badge honest — it counts the
 * tasks a user would actually see waiting.
 */
export function unblockCounts(tasks: Task[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const task of tasks) {
    for (const blocker of task.blocked_by) {
      counts.set(blocker.id, (counts.get(blocker.id) ?? 0) + 1);
    }
  }
  return counts;
}
