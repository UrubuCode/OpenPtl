export type NoteColor =
  | "yellow"
  | "blue"
  | "green"
  | "pink"
  | "purple"
  | "red"
  | "orange"
  | "cyan"
  | "default";

export interface Note {
  id: string;
  title: string;
  content: string;
  color: NoteColor;
  created_at: number;
  updated_at: number;
  pinned: boolean;
}
