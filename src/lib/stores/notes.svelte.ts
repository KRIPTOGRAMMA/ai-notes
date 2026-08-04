import { api } from "../api/tauri";
import { runGuarded } from "../guard";
import type { Note, CreateNotePayload, UpdateNotePayload } from "../types";

let notes: Note[] = $state([]);
let deletedNotes: Note[] = $state([]);
let error: string | null = $state(null);
// The "open this note" signal, set from global search; Notes.svelte reacts through
// an $effect and selects the note in the editor.
let focusNoteId: string | null = $state(null);
let dailyRequested: number = $state(0); // an increment acts as the signal

// See tasks.svelte.ts for why this wrapper stays local instead of moving into
// guard.ts: it closes over the store's own `error` rune.
async function guard<T>(op: () => Promise<T>, fallback: T): Promise<T> {
  const r = await runGuarded(op);
  if (r.ok) {
    error = null;
    return r.value;
  }
  error = r.error;
  return fallback;
}

export const noteStore = {
  get notes() { return notes; },
  get deletedNotes() { return deletedNotes; },
  get error() { return error; },
  clearError() { error = null; },
  get focusNoteId() { return focusNoteId; },
  requestFocus(id: string) { focusNoteId = id; },
  clearFocus() { focusNoteId = null; },
  get dailyRequested() { return dailyRequested; },
  requestDaily() { dailyRequested++; },

  async load() {
    notes = await guard(() => api.getNotes(), notes);
  },

  async create(payload: CreateNotePayload): Promise<Note | null> {
    const note = await guard(() => api.createNote(payload), null);
    if (note) await noteStore.load();
    return note;
  },

  // Not routed through guard(): the sentinel below must be told apart from a real
  // failure before the error reaches the banner.
  async update(id: string, patch: UpdateNotePayload) {
    const r = await runGuarded(() => api.updateNote(id, patch));
    if (!r.ok) {
      // Autosave racing deletion: the note was deleted while this save was still in
      // flight (an 800ms debounce). The backend sends a sentinel — we ignore it
      // quietly, the list is already up to date thanks to a parallel load().
      if (r.error.includes("__NOTE_DELETED__")) return;
      error = r.error;
      return;
    }
    error = null;
    await noteStore.load();
  },

  async remove(id: string) {
    const ok = await guard(async () => { await api.deleteNote(id); return true; }, false);
    if (ok) await noteStore.load();
  },

  async loadDeleted() {
    deletedNotes = await guard(() => api.getDeletedNotes(), deletedNotes);
  },

  async restore(id: string) {
    if (await guard(async () => { await api.restoreNote(id); return true; }, false)) {
      await noteStore.loadDeleted();
      await noteStore.load();
    }
  },

  async purge(id: string) {
    if (await guard(async () => { await api.purgeDeletedNote(id); return true; }, false)) {
      await noteStore.loadDeleted();
    }
  },
};
