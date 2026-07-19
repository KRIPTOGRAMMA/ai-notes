import { api } from "../api/tauri";
import type { Note, CreateNotePayload, UpdateNotePayload } from "../types";

let notes: Note[] = $state([]);
let error: string | null = $state(null);
// Сигнал «открыть эту заметку» — ставится из глобального поиска, Notes.svelte
// реагирует через $effect и выбирает заметку в редакторе.
let focusNoteId: string | null = $state(null);
let dailyRequested: number = $state(0); // инкремент как сигнал

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const noteStore = {
  get notes() { return notes; },
  get error() { return error; },
  clearError() { error = null; },
  get focusNoteId() { return focusNoteId; },
  requestFocus(id: string) { focusNoteId = id; },
  clearFocus() { focusNoteId = null; },
  get dailyRequested() { return dailyRequested; },
  requestDaily() { dailyRequested++; },

  async load() {
    try {
      notes = await api.getNotes();
    } catch (e) {
      error = describeError(e);
    }
  },

  async create(payload: CreateNotePayload): Promise<Note | null> {
    try {
      const note = await api.createNote(payload);
      await noteStore.load();
      return note;
    } catch (e) {
      error = describeError(e);
      return null;
    }
  },

  async update(id: string, patch: UpdateNotePayload) {
    try {
      await api.updateNote(id, patch);
      await noteStore.load();
    } catch (e) {
      // Гонка автосейва с удалением: заметку успели удалить, пока это
      // сохранение ещё летело (debounce 800мс). Бэкенд шлёт сентинел —
      // тихо игнорируем, список уже актуален через параллельный load().
      if (typeof e === "string" && e.includes("__NOTE_DELETED__")) return;
      error = describeError(e);
    }
  },

  async remove(id: string) {
    try {
      await api.deleteNote(id);
      await noteStore.load();
    } catch (e) {
      error = describeError(e);
    }
  },
};
