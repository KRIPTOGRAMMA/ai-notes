import { api } from "../api/tauri";
import type { PinnedItem } from "../types";
import { runGuarded } from "../guard";

// v0.9.33: «быстрый слот» — одна закреплённая задача или заметка под
// глобальным хоткеем. Стор нужен обеим вьюхам сразу: закрепить можно и из
// Задач, и из Заметок, а слот один — закрепление задачи должно снять
// подсветку с закреплённой ранее заметки, и наоборот.
let item: PinnedItem | null = $state(null);
let error: string | null = $state(null);

export const pinnedStore = {
  get item() { return item; },
  get error() { return error; },
  clearError() { error = null; },

  // Закреплено ли именно это. Проверяем и вид, и id: id генерируются
  // независимо у задач и заметок, поэтому одного id мало.
  is(kind: "task" | "note", id: string): boolean {
    return item?.kind === kind && item.id === id;
  },

  async load() {
    const r = await runGuarded(() => api.getPinnedItem());
    if (r.ok) { item = r.value; error = null; }
    else error = r.error;
  },

  // Повторное нажатие на закреплённом — открепление: одна кнопка вместо двух.
  async toggle(kind: "task" | "note", id: string) {
    const unpin = pinnedStore.is(kind, id);
    const r = await runGuarded(() =>
      api.setPinnedItem(unpin ? null : kind, unpin ? null : id)
    );
    if (!r.ok) { error = r.error; return; }
    error = null;
    await pinnedStore.load();
  },
};
