// A shared wrapper around backend calls for the stores.
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
