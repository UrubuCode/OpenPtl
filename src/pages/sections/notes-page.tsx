import { useMemo, useState } from "react";
import { Pin, PinOff, Plus, Search, StickyNote, Trash2, Pencil, X } from "lucide-react";
import Editor from "@monaco-editor/react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AppConfirmDialog } from "@/components/ui/app-dialog";
import { useNotesStore } from "@/store/notes-store";
import { useT } from "@/langs";
import type { Note, NoteColor } from "@/types/notes";

// ─── color tokens ────────────────────────────────────────────────────────────

const COLOR_BORDER: Record<NoteColor, string> = {
  default: "border-border/50",
  yellow:  "border-yellow-500/50",
  blue:    "border-blue-500/50",
  green:   "border-emerald-500/50",
  pink:    "border-pink-500/50",
  purple:  "border-purple-500/50",
  red:     "border-red-500/50",
  orange:  "border-orange-500/50",
  cyan:    "border-cyan-500/50",
};

const COLOR_BG: Record<NoteColor, string> = {
  default: "bg-card/60",
  yellow:  "bg-yellow-500/10",
  blue:    "bg-blue-500/10",
  green:   "bg-emerald-500/10",
  pink:    "bg-pink-500/10",
  purple:  "bg-purple-500/10",
  red:     "bg-red-500/10",
  orange:  "bg-orange-500/10",
  cyan:    "bg-cyan-500/10",
};

const COLOR_SWATCH: Record<NoteColor, string> = {
  default: "bg-zinc-600",
  yellow:  "bg-yellow-400",
  blue:    "bg-blue-400",
  green:   "bg-emerald-400",
  pink:    "bg-pink-400",
  purple:  "bg-purple-400",
  red:     "bg-red-400",
  orange:  "bg-orange-400",
  cyan:    "bg-cyan-400",
};

const COLOR_ACCENT: Record<NoteColor, string> = {
  default: "text-zinc-300",
  yellow:  "text-yellow-300",
  blue:    "text-blue-300",
  green:   "text-emerald-300",
  pink:    "text-pink-300",
  purple:  "text-purple-300",
  red:     "text-red-300",
  orange:  "text-orange-300",
  cyan:    "text-cyan-300",
};

const ALL_COLORS: NoteColor[] = [
  "default", "yellow", "blue", "green", "pink", "purple", "red", "orange", "cyan",
];

// ─── mention renderer ────────────────────────────────────────────────────────

function renderContent(content: string, allNotes: Note[], onMentionClick: (id: string) => void) {
  const parts = content.split(/(@\S+)/g);
  return parts.map((part, i) => {
    if (part.startsWith("@")) {
      const query = part.slice(1).toLowerCase();
      const matched = allNotes.find((n) => n.title.toLowerCase() === query);
      if (matched) {
        return (
          <button
            key={i}
            type="button"
            className={`inline font-medium underline underline-offset-2 ${COLOR_ACCENT[matched.color]} hover:opacity-80`}
            onClick={(e) => { e.stopPropagation(); onMentionClick(matched.id); }}
          >
            {part}
          </button>
        );
      }
      return <span key={i} className="text-muted-foreground/60">{part}</span>;
    }
    return <span key={i}>{part}</span>;
  });
}

// ─── NoteFormModal ───────────────────────────────────────────────────────────

interface NoteFormModalProps {
  open: boolean;
  initial?: Note;
  onClose: () => void;
}

function NoteFormModal({ open, initial, onClose }: NoteFormModalProps) {
  const t = useT();
  const tn = t.notes;
  const createNote = useNotesStore((s) => s.createNote);
  const updateNote = useNotesStore((s) => s.updateNote);

  const [title, setTitle] = useState(initial?.title ?? "");
  const [content, setContent] = useState(initial?.content ?? "");
  const [color, setColor] = useState<NoteColor>(initial?.color ?? "default");

  if (!open) return null;

  function handleClose() {
    setTitle(initial?.title ?? "");
    setContent(initial?.content ?? "");
    setColor(initial?.color ?? "default");
    onClose();
  }

  function handleSave() {
    const trimmedTitle = title.trim() || "Note";
    if (initial) {
      updateNote(initial.id, { title: trimmedTitle, content, color });
    } else {
      createNote(trimmedTitle, content, color);
    }
    handleClose();
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Escape") handleClose();
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      handleSave();
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
      onKeyDown={handleKeyDown}
    >
      <div
        className={`flex w-full max-w-3xl flex-col rounded-2xl border shadow-2xl overflow-hidden ${COLOR_BORDER[color]} ${COLOR_BG[color]}`}
        style={{ height: "min(80vh, 680px)", background: "hsl(228 12% 9%)" }}
      >
        {/* header */}
        <div className={`flex items-center gap-3 border-b px-4 py-3 ${COLOR_BORDER[color]}`}>
          <Input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={tn.titlePlaceholder}
            className={`h-8 flex-1 border-0 bg-transparent text-sm font-semibold shadow-none focus-visible:ring-0 ${COLOR_ACCENT[color]} placeholder:text-muted-foreground/40`}
          />

          {/* color swatches */}
          <div className="flex items-center gap-1.5">
            {ALL_COLORS.map((c) => (
              <button
                key={c}
                type="button"
                title={tn.colors[c]}
                onClick={() => setColor(c)}
                className={`h-4 w-4 rounded-full transition-all ${COLOR_SWATCH[c]} ${
                  color === c
                    ? "ring-2 ring-offset-1 ring-offset-background ring-white/60 scale-125"
                    : "opacity-50 hover:opacity-90 hover:scale-110"
                }`}
              />
            ))}
          </div>

          {/* actions */}
          <div className="flex items-center gap-1 ml-2">
            <button
              type="button"
              className="rounded-md bg-cyan-600 px-3 py-1 text-xs font-medium text-white hover:bg-cyan-500"
              onClick={handleSave}
            >
              {tn.drawer.save}
            </button>
            <button
              type="button"
              className="rounded-md p-1.5 text-muted-foreground hover:bg-white/10 hover:text-foreground"
              onClick={handleClose}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* monaco editor */}
        <div className="flex-1 min-h-0">
          <Editor
            height="100%"
            language="markdown"
            theme="vs-dark"
            value={content}
            onChange={(value) => setContent(value ?? "")}
            options={{
              minimap: { enabled: false },
              fontSize: 13,
              lineNumbers: "off",
              wordWrap: "on",
              scrollBeyondLastLine: false,
              renderLineHighlight: "none",
              overviewRulerLanes: 0,
              hideCursorInOverviewRuler: true,
              scrollbar: { vertical: "auto", horizontal: "hidden" },
              padding: { top: 16, bottom: 16 },
              fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
              lineHeight: 22,
              glyphMargin: false,
              folding: false,
              lineDecorationsWidth: 16,
              contextmenu: false,
            }}
          />
        </div>

        {/* footer hint */}
        <div className={`flex items-center justify-between border-t px-4 py-2 ${COLOR_BORDER[color]}`}>
          <p className="text-[10px] text-muted-foreground/40">{tn.mentionHint}</p>
          <p className="text-[10px] text-muted-foreground/40">Ctrl+S {tn.drawer.save} · Esc {tn.drawer.cancel}</p>
        </div>
      </div>
    </div>
  );
}

// ─── NoteCard ────────────────────────────────────────────────────────────────

interface NoteCardProps {
  note: Note;
  allNotes: Note[];
  onEdit: (note: Note) => void;
  onDelete: (note: Note) => void;
  onMentionClick: (id: string) => void;
}

function NoteCard({ note, allNotes, onEdit, onDelete, onMentionClick }: NoteCardProps) {
  const t = useT();
  const tn = t.notes;
  const togglePin = useNotesStore((s) => s.togglePin);

  return (
    <div
      className={`group relative flex flex-col rounded-xl border p-4 transition-shadow hover:shadow-md ${COLOR_BG[note.color]} ${COLOR_BORDER[note.color]}`}
    >
      {/* pin indicator */}
      {note.pinned && (
        <span className={`absolute right-3 top-3 h-2 w-2 rounded-full ${COLOR_SWATCH[note.color]} opacity-80`} />
      )}

      {/* title */}
      <p className={`mb-2 text-sm font-semibold leading-tight ${COLOR_ACCENT[note.color]}`}>{note.title}</p>

      {/* content */}
      <div className="flex-1 text-xs text-muted-foreground whitespace-pre-wrap leading-relaxed line-clamp-6 break-words">
        {renderContent(note.content, allNotes, onMentionClick)}
      </div>

      {/* timestamp */}
      <p className="mt-3 text-[10px] text-muted-foreground/40">
        {new Date(note.updated_at).toLocaleDateString(undefined, {
          day: "2-digit",
          month: "short",
          year: "numeric",
        })}
      </p>

      {/* actions */}
      <div className="mt-2 flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        <button
          type="button"
          title={note.pinned ? tn.unpin : tn.pin}
          className="rounded-md p-1.5 text-muted-foreground hover:bg-white/10 hover:text-foreground"
          onClick={() => togglePin(note.id)}
        >
          {note.pinned ? <PinOff className="h-3.5 w-3.5" /> : <Pin className="h-3.5 w-3.5" />}
        </button>
        <button
          type="button"
          title={tn.edit}
          className="rounded-md p-1.5 text-muted-foreground hover:bg-white/10 hover:text-foreground"
          onClick={() => onEdit(note)}
        >
          <Pencil className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          title={tn.remove}
          className="rounded-md p-1.5 text-muted-foreground hover:bg-red-500/20 hover:text-red-400"
          onClick={() => onDelete(note)}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}

// ─── NotesPage ───────────────────────────────────────────────────────────────

export function NotesPage() {
  const t = useT();
  const tn = t.notes;
  const notes = useNotesStore((s) => s.notes);
  const deleteNote = useNotesStore((s) => s.deleteNote);

  const [search, setSearch] = useState("");
  const [formOpen, setFormOpen] = useState(false);
  const [editingNote, setEditingNote] = useState<Note | undefined>(undefined);
  const [deletingNote, setDeletingNote] = useState<Note | null>(null);
  const [highlightId, setHighlightId] = useState<string | null>(null);

  const today = useMemo(() => {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d.getTime();
  }, []);

  const stats = useMemo(() => [
    { label: tn.statsTotal, value: String(notes.length), color: "text-foreground" },
    { label: tn.statsPinned, value: String(notes.filter((n) => n.pinned).length), color: "text-yellow-400" },
    { label: tn.statsColored, value: String(notes.filter((n) => n.color !== "default").length), color: "text-primary" },
    { label: tn.statsUpdatedToday, value: String(notes.filter((n) => new Date(n.updated_at).setHours(0,0,0,0) >= today).length), color: "text-green-400" },
  ], [notes, tn.statsColored, tn.statsPinned, tn.statsTotal, tn.statsUpdatedToday, today]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return notes.filter(
      (n) => n.title.toLowerCase().includes(q) || n.content.toLowerCase().includes(q),
    );
  }, [notes, search]);

  const pinned = filtered.filter((n) => n.pinned);
  const unpinned = filtered.filter((n) => !n.pinned);

  function handleEdit(note: Note) {
    setEditingNote(note);
    setFormOpen(true);
  }

  function handleMentionClick(id: string) {
    setHighlightId(id);
    setTimeout(() => {
      document
        .getElementById(`note-card-${id}`)
        ?.scrollIntoView({ behavior: "smooth", block: "center" });
    }, 50);
    setTimeout(() => setHighlightId(null), 2000);
  }

  return (
    <div className="flex-1 p-6 space-y-6 h-full overflow-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-foreground">{tn.title}</h1>
          <p className="text-sm text-muted-foreground mt-0.5">{tn.subtitle}</p>
        </div>
        <Button
          size="sm"
          className="gap-2"
          onClick={() => { setEditingNote(undefined); setFormOpen(true); }}
        >
          <Plus className="h-4 w-4" />
          {tn.newNote}
        </Button>
      </div>

      {/* Search */}
      <div className="relative max-w-sm">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={tn.searchPlaceholder}
          className="h-9 w-full rounded-lg border border-border bg-secondary/50 pl-9 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        />
      </div>

      {/* Stats */}
      <div className="grid grid-cols-4 gap-3">
        {stats.map((stat) => (
          <div key={stat.label} className="rounded-lg border border-border/40 bg-card p-3">
            <p className="text-[10px] uppercase tracking-wider text-muted-foreground">{stat.label}</p>
            <p className={`text-2xl font-semibold mt-1 ${stat.color}`}>{stat.value}</p>
          </div>
        ))}
      </div>

      {/* Notes */}
      {notes.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-4 py-16 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-border/40 bg-card/60">
            <StickyNote className="h-7 w-7 text-muted-foreground/40" />
          </div>
          <div>
            <p className="text-sm font-medium text-foreground">{tn.emptyTitle}</p>
            <p className="mt-1 max-w-xs text-xs text-muted-foreground">{tn.emptyDescription}</p>
          </div>
          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            onClick={() => { setEditingNote(undefined); setFormOpen(true); }}
          >
            <Plus className="h-3.5 w-3.5" />
            {tn.addFirst}
          </Button>
        </div>
      ) : (
        <div className="space-y-6">
          {pinned.length > 0 && (
            <section>
              <p className="mb-3 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                {tn.pinned}
              </p>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {pinned.map((n) => (
                  <div
                    key={n.id}
                    id={`note-card-${n.id}`}
                    className={`transition-all duration-500 ${highlightId === n.id ? "ring-2 ring-cyan-400/60 rounded-xl" : ""}`}
                  >
                    <NoteCard
                      note={n}
                      allNotes={notes}
                      onEdit={handleEdit}
                      onDelete={setDeletingNote}
                      onMentionClick={handleMentionClick}
                    />
                  </div>
                ))}
              </div>
            </section>
          )}

          {unpinned.length > 0 && (
            <section>
              {pinned.length > 0 && (
                <p className="mb-3 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
                  {tn.all}
                </p>
              )}
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {unpinned.map((n) => (
                  <div
                    key={n.id}
                    id={`note-card-${n.id}`}
                    className={`transition-all duration-500 ${highlightId === n.id ? "ring-2 ring-cyan-400/60 rounded-xl" : ""}`}
                  >
                    <NoteCard
                      note={n}
                      allNotes={notes}
                      onEdit={handleEdit}
                      onDelete={setDeletingNote}
                      onMentionClick={handleMentionClick}
                    />
                  </div>
                ))}
              </div>
            </section>
          )}
        </div>
      )}

      <NoteFormModal
        open={formOpen}
        initial={editingNote}
        onClose={() => { setFormOpen(false); setEditingNote(undefined); }}
      />

      <AppConfirmDialog
        open={deletingNote !== null}
        title={tn.deleteConfirmTitle}
        message={tn.deleteConfirmMessage.replace("{title}", deletingNote?.title ?? "")}
        confirmLabel={tn.deleteConfirmAction}
        busy={false}
        onCancel={() => setDeletingNote(null)}
        onConfirm={() => {
          if (deletingNote) {
            deleteNote(deletingNote.id);
            setDeletingNote(null);
          }
        }}
      />
    </div>
  );
}
