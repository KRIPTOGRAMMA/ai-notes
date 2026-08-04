import { describe, it, expect } from "vitest";
import { unblockCounts } from "./blockers";
import type { Task, Blocker } from "./types";

// Only the fields the inversion reads; the rest of Task is irrelevant here.
function task(id: string, blockedBy: Blocker[] = []): Task {
  return { id, title: id, blocked_by: blockedBy } as unknown as Task;
}
const by = (id: string): Blocker => ({ id, title: id });

describe("unblockCounts", () => {
  it("counts how many tasks each blocker holds up", () => {
    const counts = unblockCounts([
      task("a"),
      task("b", [by("a")]),
      task("c", [by("a")]),
      task("d", [by("b")]),
    ]);
    expect(counts.get("a")).toBe(2);
    expect(counts.get("b")).toBe(1);
  });

  // A miss means "blocks nothing", so the caller never has to tell an absent key
  // from a zero — and a badge is never rendered with "unblocks 0".
  it("a task that blocks nothing is absent from the map", () => {
    const counts = unblockCounts([task("a"), task("b", [by("a")])]);
    expect(counts.has("b")).toBe(false);
    expect(counts.get("b") ?? 0).toBe(0);
  });

  it("counts a task blocked by several blockers once for each of them", () => {
    const counts = unblockCounts([task("c", [by("a"), by("b")])]);
    expect(counts.get("a")).toBe(1);
    expect(counts.get("b")).toBe(1);
  });

  it("an empty list gives an empty map", () => {
    expect(unblockCounts([]).size).toBe(0);
  });

  // The backend already drops completed, hidden and trashed blockers from
  // blocked_by (OPEN_BLOCKER), so a finished blocker simply stops being counted —
  // the badge disappears on its own without any extra bookkeeping here.
  it("a blocker no longer listed by anyone drops out of the map", () => {
    const before = unblockCounts([task("b", [by("a")])]);
    expect(before.get("a")).toBe(1);
    const after = unblockCounts([task("b")]);
    expect(after.has("a")).toBe(false);
  });
});
