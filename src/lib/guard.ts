// Общая обёртка вокруг вызовов бэкенда для сторов (v0.9.25).
//
// Вынесена из tasks.svelte.ts отдельным чистым модулем сознательно: vitest
// в этом проекте настроен только на чистые ts-модули (vitest.config.ts,
// include: src/**/*.test.ts, без svelte-плагина), поэтому логику, живущую
// рядом с $state-рунами, юнит-тестами не покрыть. Здесь же — обычные
// функции, и поведение ошибок проверяется тестами.

export function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export type GuardResult<T> = { ok: true; value: T } | { ok: false; error: string };

// Выполняет операцию, приводя любую ошибку к строке для показа пользователю.
// Вызывающий сам решает, что делать с результатом — стор кладёт error в
// $state, откуда его рендерит вью.
export async function runGuarded<T>(op: () => Promise<T>): Promise<GuardResult<T>> {
  try {
    return { ok: true, value: await op() };
  } catch (e) {
    return { ok: false, error: describeError(e) };
  }
}
