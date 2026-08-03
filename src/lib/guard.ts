// A shared wrapper around backend calls for the stores.
//
// Where a store's error is rendered (the contract, v0.9.68):
//   - Stores whose data is visible from everywhere (taskStore, noteStore,
//     pinnedStore) render in the App.svelte banner.
//   - Screen-local stores render an inline .alert on the screen that owns them:
//     categoryStore/statusStore in Settings, projectStore/smartListStore in
//     Tasks, routineStore in RoutinesModal.
// A store that sets `error` with no render site is a bug — the failure reaches
// nobody. Every store method must also clear `error` on success, or the first
// failure pins the banner until the window is reloaded.
//
// Deliberately extracted from tasks.svelte.ts into its own pure module: vitest in
// this project is configured for pure ts modules only (vitest.config.ts, include:
// src/**/*.test.ts, with no svelte plugin), so logic living next to $state runes
// cannot be covered by unit tests. Here these are ordinary functions and the error
// behaviour is verified by tests.

export function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export type GuardResult<T> = { ok: true; value: T } | { ok: false; error: string };

// Runs an operation, reducing any error to a string for display to the user. What
// to do with the result is the caller's decision — the store puts the error into
// $state, from where the view renders it.
export async function runGuarded<T>(op: () => Promise<T>): Promise<GuardResult<T>> {
  try {
    return { ok: true, value: await op() };
  } catch (e) {
    return { ok: false, error: describeError(e) };
  }
}
