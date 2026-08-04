import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { loadUiState, saveUiState, restoreValid, restoreOneOf } from "./uistate";

const LS_KEY = "ui_state";

// A localStorage of our own instead of jsdom's. Node intercepts the name as its
// own experimental API and, without --localstorage-file, leaves it undefined —
// shadowing jsdom's implementation. Patching the config was not an option (the
// testing setup is deliberately left alone), and a stub is honest anyway: these
// tests care about the module's behaviour, not about jsdom's storage.
class MemoryStorage {
  private map = new Map<string, string>();
  getItem(k: string) { return this.map.get(k) ?? null; }
  setItem(k: string, v: string) { this.map.set(k, String(v)); }
  removeItem(k: string) { this.map.delete(k); }
  clear() { this.map.clear(); }
  key(i: number) { return [...this.map.keys()][i] ?? null; }
  get length() { return this.map.size; }
}

beforeEach(() => {
  vi.stubGlobal("localStorage", new MemoryStorage());
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("loadUiState / saveUiState", () => {
  it("what was saved comes back", () => {
    saveUiState({ view: "notes", taskViewMode: "board" });
    expect(loadUiState()).toEqual({ view: "notes", taskViewMode: "board" });
  });

  it("saving merges instead of replacing", () => {
    saveUiState({ view: "notes" });
    saveUiState({ taskViewMode: "board" });
    expect(loadUiState()).toEqual({ view: "notes", taskViewMode: "board" });
  });

  it("nothing saved yet is an empty state, not a crash", () => {
    expect(loadUiState()).toEqual({});
  });

  // The regression the try/catch exists for: a half-written or hand-edited value
  // must not stop the app from starting.
  it("broken JSON yields an empty state", () => {
    localStorage.setItem(LS_KEY, "{not json");
    expect(loadUiState()).toEqual({});
  });

  // JSON.parse returns these happily, and none can be spread into state.
  it("valid JSON that is not an object yields an empty state", () => {
    for (const raw of ['"a string"', "42", "null", "[1,2]"]) {
      localStorage.setItem(LS_KEY, raw);
      expect(loadUiState()).toEqual({});
    }
  });

  it("an unavailable localStorage costs the memory, not the launch", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => { throw new Error("SecurityError"); },
      setItem: () => { throw new Error("QuotaExceededError"); },
    });
    expect(() => saveUiState({ view: "notes" })).not.toThrow();
    expect(loadUiState()).toEqual({});
  });
});

describe("restoreValid", () => {
  const exists = (id: string) => ["a", "b"].includes(id);

  it("restores a value that is still valid", () => {
    expect(restoreValid("a", exists, "fallback")).toBe("a");
  });

  // The core of the feature: a smart list, project or note deleted between two
  // launches must not leave the user on an empty screen filtered by a ghost.
  it("falls back when the saved id no longer exists", () => {
    expect(restoreValid("deleted", exists, "fallback")).toBe("fallback");
  });

  it("nothing saved gives the fallback", () => {
    expect(restoreValid(undefined, exists, "fallback")).toBe("fallback");
  });

  // null is a meaningful saved value elsewhere ("no smart list"), but here it means
  // there is nothing to restore, so it must not be handed to isValid.
  it("null gives the fallback without consulting isValid", () => {
    const isValid = vi.fn(() => true);
    expect(restoreValid(null as unknown as string, isValid, "fallback")).toBe("fallback");
    expect(isValid).not.toHaveBeenCalled();
  });
});

describe("restoreOneOf", () => {
  const MODES = ["list", "board"] as const;

  it("restores an allowed value", () => {
    expect(restoreOneOf("board", MODES, "list")).toBe("board");
  });

  // A value from an older version, or an edited one, must not become live state.
  it("rejects a value outside the set", () => {
    expect(restoreOneOf("kanban", MODES, "list")).toBe("list");
    expect(restoreOneOf(undefined, MODES, "list")).toBe("list");
  });
});
