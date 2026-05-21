import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { Note, NoteColor } from "@/types/notes";

interface NotesState {
  notes: Note[];
}

interface NotesActions {
  createNote: (title: string, content: string, color: NoteColor) => Note;
  updateNote: (id: string, patch: Partial<Pick<Note, "title" | "content" | "color" | "pinned">>) => void;
  deleteNote: (id: string) => void;
  togglePin: (id: string) => void;
}

export type NotesStore = NotesState & NotesActions;

export const useNotesStore = create<NotesStore>()(
  persist(
    (set, get) => ({
      notes: [],

      createNote(title, content, color) {
        const now = Date.now();
        const note: Note = {
          id: `note:${now}:${Math.random().toString(16).slice(2, 8)}`,
          title,
          content,
          color,
          created_at: now,
          updated_at: now,
          pinned: false,
        };
        set((state) => ({ notes: [note, ...state.notes] }));
        return note;
      },

      updateNote(id, patch) {
        set((state) => ({
          notes: state.notes.map((n) =>
            n.id === id ? { ...n, ...patch, updated_at: Date.now() } : n,
          ),
        }));
      },

      deleteNote(id) {
        set((state) => ({ notes: state.notes.filter((n) => n.id !== id) }));
      },

      togglePin(id) {
        const note = get().notes.find((n) => n.id === id);
        if (!note) return;
        set((state) => ({
          notes: state.notes.map((n) =>
            n.id === id ? { ...n, pinned: !n.pinned, updated_at: Date.now() } : n,
          ),
        }));
      },
    }),
    {
      name: "openptl.notes",
      storage: createJSONStorage(() => localStorage),
    },
  ),
);
