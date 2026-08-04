// Where you were last time (v0.9.79).
//
// Until now only the theme survived a restart, so every launch began with
// restoring context by hand: the active screen, List/Board, the chosen smart
// list, the project filter, the Settings tab, the last open note.
//
// This is window state, not a setting: it belongs to this machine and this
// window, there is nothing to sync between devices, and it must not travel in a
// DB export. Hence localStorage rather than the `settings` table — the same
// reasoning, and the same try/catch, as theme.ts (localStorage throws in private
// mode and when storage is full).
//
// Deliberately NOT remembered: the History/Trash sub-views. Opening the app
// straight into the Trash is disorienting — a destructive-looking screen with no
// memory of asking for it.

const LS_KEY = "ui_state";

export interface UiState {
  view: string;
  taskViewMode: string;
  smartListId: string | null;
  projectFilter: string;
  settingsTab: string;
  dashboardAppPeriod: number;
  noteId: string | null;
}

// A partial value is the normal case, not an error: a state written by an older
// version simply lacks the newer keys.
export type StoredUiState = Partial<UiState>;

/**
 * Reads the saved state. Any failure yields {} rather than throwing: a corrupt or
 * unavailable store must cost the user the memory of where they were, nothing
 * more. A blank app is a far better outcome than an app that will not start.
 */
export function loadUiState(): StoredUiState {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    // JSON.parse happily returns a string, a number or null — none of which can be
    // spread into state. Only a plain object is usable.
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    return parsed as StoredUiState;
  } catch {
    return {};
  }
}

/** Merges a patch into the saved state. Silent on failure, exactly like theme.ts. */
export function saveUiState(patch: StoredUiState): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify({ ...loadUiState(), ...patch }));
  } catch {
    // private mode, or the quota is full — losing the cursor position is not worth
    // an error the user has to deal with
  }
}

/**
 * Restores a value only if it is still valid, otherwise falls back.
 *
 * This is the whole point of the module rather than a nicety. A remembered smart
 * list, project or note can be deleted between two launches; restoring it blindly
 * would open an empty screen with a filter nothing matches — and the user has no
 * way to tell that from "I have no tasks". Falling back silently is right here:
 * there is nothing to report, the saved choice simply no longer exists.
 */
export function restoreValid<T, F = T>(
  saved: T | null | undefined,
  isValid: (v: T) => boolean,
  fallback: F,
): T | F {
  if (saved === undefined || saved === null) return fallback;
  return isValid(saved) ? saved : fallback;
}

/** Restores a value from a fixed set of allowed ones (List/Board, a Settings tab). */
export function restoreOneOf<T extends string>(saved: string | undefined, allowed: readonly T[], fallback: T): T {
  return allowed.includes(saved as T) ? (saved as T) : fallback;
}
