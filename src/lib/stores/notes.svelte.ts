import { api } from "../api/tauri";
import type { Note, CreateNotePayload, UpdateNotePayload } from "../types";

let notes: Note[] = $state([]);
let error: string | null = $state(null);

function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Неизвестная ошибка";
}

export const noteStore = {
  get notes() { return notes; },
  get error() { return error; },
  clearError() { error = null; },

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
