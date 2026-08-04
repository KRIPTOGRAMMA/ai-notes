import { describe, it, expect } from "vitest";
import { isTypingTarget, actionForKey, nextIndex, reconcileIndex } from "./listnav";

describe("isTypingTarget", () => {
  it("recognises the fields the user types in", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(isTypingTarget(document.createElement(tag))).toBe(true);
    }
  });

  it("treats contenteditable as typing: the note editor is neither input nor textarea", () => {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "true");
    document.body.appendChild(el);
    expect(isTypingTarget(el)).toBe(true);

    // The caret in CodeMirror sits on a node *inside* the editable region, not on
    // the region itself, so inheritance has to be honoured.
    const inner = document.createElement("span");
    el.appendChild(inner);
    expect(isTypingTarget(inner)).toBe(true);
    el.remove();
  });

  it("contenteditable='false' is not typing", () => {
    const el = document.createElement("div");
    el.setAttribute("contenteditable", "false");
    document.body.appendChild(el);
    expect(isTypingTarget(el)).toBe(false);
    el.remove();
  });

  it("a plain element and a missing focus are not typing", () => {
    expect(isTypingTarget(document.createElement("li"))).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe("actionForKey", () => {
  it("maps letters and arrows to the same movement", () => {
    expect(actionForKey({ key: "j" })).toBe("down");
    expect(actionForKey({ key: "ArrowDown" })).toBe("down");
    expect(actionForKey({ key: "k" })).toBe("up");
    expect(actionForKey({ key: "ArrowUp" })).toBe("up");
  });

  it("maps the action keys", () => {
    expect(actionForKey({ key: "Enter" })).toBe("open");
    expect(actionForKey({ key: " " })).toBe("complete");
    expect(actionForKey({ key: "Delete" })).toBe("delete");
    expect(actionForKey({ key: "Escape" })).toBe("escape");
  });

  // The regression that matters: Ctrl+Shift+J opens the quick slot (v0.9.33) and
  // Ctrl+N/Ctrl+K are global. If a modified key still mapped to an action, the
  // list would act on the same keystroke that triggers the hotkey.
  it("ignores modified keys so global hotkeys keep working", () => {
    expect(actionForKey({ key: "j", ctrlKey: true })).toBe(null);
    expect(actionForKey({ key: "j", metaKey: true })).toBe(null);
    expect(actionForKey({ key: "k", altKey: true })).toBe(null);
    expect(actionForKey({ key: "ArrowDown", ctrlKey: true })).toBe(null);
  });

  // An overlay above a still-mounted view handled this key already; both listeners
  // are on <svelte:window>, so the list would otherwise act on it too.
  it("ignores a key an overlay already handled", () => {
    expect(actionForKey({ key: "Escape", defaultPrevented: true })).toBe(null);
    expect(actionForKey({ key: "Enter", defaultPrevented: true })).toBe(null);
    expect(actionForKey({ key: "Escape" })).toBe("escape");
  });

  it("ignores keys that are not navigation", () => {
    expect(actionForKey({ key: "a" })).toBe(null);
    expect(actionForKey({ key: "Tab" })).toBe(null);
  });
});

describe("nextIndex", () => {
  it("the first keystroke lands on a row from either direction", () => {
    expect(nextIndex(-1, 1, 5)).toBe(0);
    expect(nextIndex(-1, -1, 5)).toBe(4);
  });

  it("moves one row at a time", () => {
    expect(nextIndex(2, 1, 5)).toBe(3);
    expect(nextIndex(2, -1, 5)).toBe(1);
  });

  // Stops rather than wraps: holding j must not silently jump back to the top.
  it("stops at both ends", () => {
    expect(nextIndex(4, 1, 5)).toBe(4);
    expect(nextIndex(0, -1, 5)).toBe(0);
  });

  it("an empty list has no cursor", () => {
    expect(nextIndex(-1, 1, 0)).toBe(-1);
    expect(nextIndex(3, 1, 0)).toBe(-1);
  });
});

describe("reconcileIndex", () => {
  // The list is re-sorted on every save; a bare index would drift onto another task.
  it("follows the row when it moves", () => {
    expect(reconcileIndex("b", ["c", "a", "b"], 0)).toBe(2);
  });

  it("keeps the position when the row is gone, landing on the next row", () => {
    // "b" was completed and left the list: the cursor stays at index 1, which is
    // now "c" — the row that followed it.
    expect(reconcileIndex("b", ["a", "c", "d"], 1)).toBe(1);
  });

  it("clamps when the removed row was the last one", () => {
    expect(reconcileIndex("c", ["a", "b"], 2)).toBe(1);
  });

  it("an emptied list drops the cursor", () => {
    expect(reconcileIndex("a", [], 0)).toBe(-1);
  });

  it("no cursor stays no cursor", () => {
    expect(reconcileIndex(null, ["a", "b"], -1)).toBe(-1);
  });
});
