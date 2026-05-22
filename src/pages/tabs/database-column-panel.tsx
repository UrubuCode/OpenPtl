import {
  RefreshCw,
  Table2,
  Trash2,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";

import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { DbColumnDef, DbTableDefinition, DbTableInfo } from "@/types/openptl";

// ─── Types ────────────────────────────────────────────────────────────────────

interface ColRow {
  id: string;
  orig: DbColumnDef | null;
  name: string;
  base: string;
  length: string;
  nullable: boolean;
  defaultVal: string;
  extra: string;
  comment: string;
  isPrimary: boolean;
  isNew: boolean;
  isDeleted: boolean;
}

type EditField = "name" | "base" | "length" | "defaultVal" | "extra" | "comment";

interface EditingCell {
  rowId: string;
  field: EditField;
}

interface ColumnStructurePanelProps {
  tbl: DbTableInfo;
  def: DbTableDefinition | null;
  sessionId: string | null;
  database: string | null;
  schema: string;
  isMysql: boolean;
  onRefresh: () => void;
  onQueryError: (msg: string) => void;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const MYSQL_TYPES_GROUPS: { group: string; types: string[] }[] = [
  { group: "Integer", types: ["TINYINT", "SMALLINT", "MEDIUMINT", "INT", "BIGINT"] },
  { group: "Decimal", types: ["DECIMAL", "FLOAT", "DOUBLE"] },
  { group: "String", types: ["CHAR", "VARCHAR", "TINYTEXT", "TEXT", "MEDIUMTEXT", "LONGTEXT"] },
  { group: "Date/Time", types: ["DATE", "DATETIME", "TIMESTAMP", "TIME", "YEAR"] },
  { group: "Binary", types: ["BINARY", "VARBINARY", "TINYBLOB", "BLOB", "MEDIUMBLOB", "LONGBLOB"] },
  { group: "Other", types: ["BIT", "BOOLEAN", "ENUM", "SET", "JSON"] },
];

const TYPES_WITHOUT_LENGTH = new Set([
  "TINYTEXT", "TEXT", "MEDIUMTEXT", "LONGTEXT",
  "TINYBLOB", "BLOB", "MEDIUMBLOB", "LONGBLOB",
  "DATE", "DATETIME", "TIMESTAMP", "TIME", "YEAR",
  "JSON", "BOOLEAN", "GEOMETRY", "ENUM", "SET",
]);

// ─── Helpers ─────────────────────────────────────────────────────────────────

function parseDataType(dt: string): { base: string; length: string } {
  const m = dt.match(/^([a-zA-Z]+)\((.+)\)$/);
  if (m) return { base: m[1].toUpperCase(), length: m[2] };
  return { base: dt.toUpperCase(), length: "" };
}

function buildDataType(base: string, length: string): string {
  if (length.trim() && !TYPES_WITHOUT_LENGTH.has(base.toUpperCase())) {
    return `${base}(${length})`;
  }
  return base;
}

function colRowFromDef(def: DbColumnDef): ColRow {
  const { base, length } = parseDataType(def.data_type);
  return {
    id: crypto.randomUUID(),
    orig: def,
    name: def.name,
    base,
    length,
    nullable: def.nullable,
    defaultVal: def.default_value ?? "",
    extra: def.extra ?? "",
    comment: def.comment ?? "",
    isPrimary: def.is_primary_key,
    isNew: false,
    isDeleted: false,
  };
}

function isDirtyRow(row: ColRow): boolean {
  if (row.isNew || !row.orig) return false;
  const o = row.orig;
  const { base, length } = parseDataType(o.data_type);
  return (
    row.name !== o.name ||
    row.base !== base ||
    row.length !== length ||
    row.nullable !== o.nullable ||
    row.defaultVal !== (o.default_value ?? "") ||
    row.extra !== (o.extra ?? "") ||
    row.comment !== (o.comment ?? "") ||
    row.isPrimary !== o.is_primary_key
  );
}

function escSql(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
}

function buildColDef(row: ColRow): string {
  const dt = buildDataType(row.base, row.length);
  const nullPart = row.nullable ? "NULL" : "NOT NULL";
  const defaultPart =
    row.defaultVal === ""
      ? ""
      : row.defaultVal.toUpperCase() === "NULL"
      ? " DEFAULT NULL"
      : ` DEFAULT '${escSql(row.defaultVal)}'`;
  const extraPart = row.extra ? ` ${row.extra}` : "";
  const commentPart = row.comment ? ` COMMENT '${escSql(row.comment)}'` : "";
  return `\`${row.name}\` ${dt} ${nullPart}${defaultPart}${extraPart}${commentPart}`;
}

function buildAlterSql(
  cols: ColRow[],
  tbl: DbTableInfo,
  database: string,
  isMysql: boolean,
): string | null {
  if (!isMysql) return null;

  const clauses: string[] = [];
  const nonDeleted = cols.filter((r) => !r.isDeleted);

  for (let i = 0; i < cols.length; i++) {
    const row = cols[i];

    if (row.isNew && row.isDeleted) continue;

    if (row.isNew && !row.isDeleted) {
      // Determine AFTER clause
      const prevNonNew = cols.slice(0, i).filter((r) => !r.isNew && !r.isDeleted);
      const afterClause =
        prevNonNew.length > 0
          ? ` AFTER \`${prevNonNew[prevNonNew.length - 1].name}\``
          : " FIRST";
      clauses.push(`  ADD COLUMN ${buildColDef(row)}${afterClause}`);
      continue;
    }

    if (!row.isNew && row.isDeleted && row.orig) {
      clauses.push(`  DROP COLUMN \`${row.orig.name}\``);
      continue;
    }

    if (!row.isNew && !row.isDeleted && row.orig && isDirtyRow(row)) {
      if (row.name !== row.orig.name) {
        clauses.push(`  CHANGE COLUMN \`${row.orig.name}\` ${buildColDef(row)}`);
      } else {
        clauses.push(`  MODIFY COLUMN ${buildColDef(row)}`);
      }
    }
  }

  if (clauses.length === 0) return null;

  void nonDeleted;
  return `ALTER TABLE \`${database}\`.\`${tbl.name}\`\n${clauses.join(",\n")};`;
}

// ─── SQL Preview Modal ────────────────────────────────────────────────────────

function SqlPreviewModal({
  sql,
  onCancel,
  onExecute,
  executing,
}: {
  sql: string;
  onCancel: () => void;
  onExecute: () => void;
  executing: boolean;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div
        className="bg-popover border border-border rounded-lg shadow-2xl w-full max-w-2xl mx-4 flex flex-col"
        style={{ maxHeight: "80vh" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border shrink-0">
          <p className="text-sm font-medium">Prévia do SQL gerado</p>
          <p className="text-xs text-muted-foreground mt-0.5">Revise e execute o ALTER TABLE abaixo.</p>
        </div>
        <div className="flex-1 overflow-auto p-4">
          <pre className="text-xs font-mono text-green-400 bg-black/40 rounded p-3 whitespace-pre-wrap">
            {sql}
          </pre>
        </div>
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-border shrink-0">
          <button
            className="px-3 py-1.5 text-xs border border-border rounded hover:bg-accent"
            onClick={onCancel}
            disabled={executing}
          >
            Cancelar
          </button>
          <button
            className="px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90 disabled:opacity-50 flex items-center gap-1.5"
            onClick={onExecute}
            disabled={executing}
          >
            {executing && <RefreshCw className="h-3 w-3 animate-spin" />}
            Executar
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Inline editable cell ─────────────────────────────────────────────────────

function EditableCell({
  value,
  editing,
  onDoubleClick,
  onCommit,
  onCancel,
  className,
  children,
}: {
  value: string;
  editing: boolean;
  onDoubleClick: () => void;
  onCommit: (val: string) => void;
  onCancel: () => void;
  className?: string;
  children?: React.ReactNode;
}) {
  const [draft, setDraft] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) {
      setDraft(value);
      setTimeout(() => inputRef.current?.focus(), 20);
    }
  }, [editing, value]);

  if (editing) {
    return (
      <input
        ref={inputRef}
        className="w-full bg-primary/10 border border-primary rounded px-1 text-xs font-mono outline-none min-w-[40px]"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => onCommit(draft)}
        onKeyDown={(e: ReactKeyboardEvent<HTMLInputElement>) => {
          if (e.key === "Enter") { e.currentTarget.blur(); }
          if (e.key === "Escape") { onCancel(); }
        }}
      />
    );
  }

  return (
    <span
      className={cn("cursor-default select-none", className)}
      onDoubleClick={onDoubleClick}
    >
      {children ?? value}
    </span>
  );
}

// ─── Type dropdown cell ───────────────────────────────────────────────────────

function TypeDropdownCell({
  value,
  editing,
  onDoubleClick,
  onCommit,
  onCancel,
}: {
  value: string;
  editing: boolean;
  onDoubleClick: () => void;
  onCommit: (val: string) => void;
  onCancel: () => void;
}) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const [search, setSearch] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const [pos, setPos] = useState({ top: 0, left: 0 });

  useEffect(() => {
    if (editing) {
      setSearch("");
      if (anchorRef.current) {
        const r = anchorRef.current.getBoundingClientRect();
        setPos({ top: r.bottom + 4, left: r.left });
      }
      setTimeout(() => searchRef.current?.focus(), 20);
    }
  }, [editing]);

  useEffect(() => {
    if (!editing) return;
    const handler = (e: MouseEvent) => {
      if (
        !anchorRef.current?.contains(e.target as Node) &&
        !dropdownRef.current?.contains(e.target as Node)
      ) onCancel();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [editing, onCancel]);

  const filtered = MYSQL_TYPES_GROUPS.map((g) => ({
    group: g.group,
    types: search
      ? g.types.filter((t) => t.toLowerCase().includes(search.toLowerCase()))
      : g.types,
  })).filter((g) => g.types.length > 0);

  const dropdown = editing ? createPortal(
    <div
      ref={dropdownRef}
      className="w-52 rounded-lg border border-border bg-popover shadow-2xl overflow-hidden"
      style={{ position: "fixed", top: pos.top, left: pos.left, zIndex: 9999 }}
    >
      <div className="p-1.5 border-b border-border">
        <input
          ref={searchRef}
          className="w-full bg-muted/40 rounded px-2 py-0.5 text-xs outline-none placeholder:text-muted-foreground/60"
          placeholder="Buscar tipo..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={(e: ReactKeyboardEvent<HTMLInputElement>) => {
            if (e.key === "Escape") { e.stopPropagation(); onCancel(); }
            if (e.key === "Enter") {
              const first = filtered[0]?.types[0];
              if (first) onCommit(first);
            }
          }}
        />
      </div>
      <div className="overflow-y-auto max-h-60">
        {filtered.map((g) => (
          <div key={g.group}>
            <div className="px-2 py-1 text-[9px] font-semibold uppercase tracking-wider text-muted-foreground/60 bg-muted/20 sticky top-0">
              {g.group}
            </div>
            {g.types.map((t) => (
              <button
                key={t}
                type="button"
                className={cn(
                  "w-full text-left px-3 py-1 text-xs font-mono hover:bg-accent transition-colors",
                  t === value && "bg-primary/10 text-primary font-semibold",
                )}
                onMouseDown={(e) => { e.preventDefault(); onCommit(t); }}
              >
                {t}
              </button>
            ))}
          </div>
        ))}
        {filtered.length === 0 && (
          <div className="px-3 py-2 text-xs text-muted-foreground">Nenhum resultado</div>
        )}
      </div>
    </div>,
    document.body,
  ) : null;

  return (
    <span ref={anchorRef} className="inline-block">
      <span
        className={cn("cursor-default select-none font-mono", editing ? "text-primary" : "text-blue-400")}
        onDoubleClick={onDoubleClick}
      >
        {value}
      </span>
      {dropdown}
    </span>
  );
}

type StructureTab = "columns" | "indexes" | "fk" | "ddl";

// ─── Main component ───────────────────────────────────────────────────────────

export function ColumnStructurePanel({
  tbl,
  def,
  sessionId,
  database,
  isMysql,
  onRefresh,
  onQueryError,
}: ColumnStructurePanelProps) {
  const [activeTab, setActiveTab] = useState<StructureTab>("columns");
  const [cols, setCols] = useState<ColRow[]>([]);
  const [editingCell, setEditingCell] = useState<EditingCell | null>(null);
  const [previewSql, setPreviewSql] = useState<string | null>(null);
  const [executing, setExecuting] = useState(false);

  // (Re-)initialize cols from def
  useEffect(() => {
    if (!def) { setCols([]); return; }
    setCols(def.columns.map(colRowFromDef));
  }, [def]);

  const setRow = useCallback((id: string, update: Partial<ColRow>) => {
    setCols((prev) => prev.map((r) => r.id === id ? { ...r, ...update } : r));
  }, []);

  const dirtyCount = cols.filter(
    (r) => !r.isDeleted && (r.isNew || isDirtyRow(r)),
  ).length + cols.filter((r) => r.isDeleted && !r.isNew).length;

  const handleApply = useCallback(() => {
    if (!database || !isMysql) return;
    const sql = buildAlterSql(cols, tbl, database, isMysql);
    if (!sql) return;
    setPreviewSql(sql);
  }, [cols, tbl, database, isMysql]);

  const handleExecute = useCallback(async () => {
    if (!previewSql || !sessionId) return;
    setExecuting(true);
    try {
      await api.dbQuery(sessionId, previewSql);
      setPreviewSql(null);
      setExecuting(false);
      onRefresh();
    } catch (err) {
      setExecuting(false);
      setPreviewSql(null);
      onQueryError(err instanceof Error ? err.message : String(err));
    }
  }, [previewSql, sessionId, onRefresh, onQueryError]);

  const handleDiscard = useCallback(() => {
    if (!def) return;
    setCols(def.columns.map(colRowFromDef));
    setEditingCell(null);
  }, [def]);

  const handleAddColumn = useCallback(() => {
    setCols((prev) => [
      ...prev,
      {
        id: crypto.randomUUID(),
        orig: null,
        name: "new_column",
        base: "VARCHAR",
        length: "255",
        nullable: true,
        defaultVal: "",
        extra: "",
        comment: "",
        isPrimary: false,
        isNew: true,
        isDeleted: false,
      },
    ]);
  }, []);

  const startEdit = useCallback((rowId: string, field: EditField) => {
    setEditingCell({ rowId, field });
  }, []);

  const commitEdit = useCallback(
    (rowId: string, field: EditField, val: string) => {
      setEditingCell(null);
      const row = cols.find((r) => r.id === rowId);
      if (!row) return;

      if (field === "base") {
        const newBase = val.toUpperCase();
        const clearLen = TYPES_WITHOUT_LENGTH.has(newBase);
        setRow(rowId, { base: newBase, length: clearLen ? "" : row.length });
      } else if (field === "name") {
        setRow(rowId, { name: val });
      } else if (field === "length") {
        setRow(rowId, { length: val });
      } else if (field === "defaultVal") {
        setRow(rowId, { defaultVal: val });
      } else if (field === "extra") {
        setRow(rowId, { extra: val });
      } else if (field === "comment") {
        setRow(rowId, { comment: val });
      }
    },
    [cols, setRow],
  );

  const cancelEdit = useCallback(() => {
    setEditingCell(null);
  }, []);

  const isEditing = (rowId: string, field: EditField) =>
    editingCell?.rowId === rowId && editingCell?.field === field;

  if (!def) {
    return (
      <div className="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground">
        <RefreshCw className="h-4 w-4 animate-spin" />
      </div>
    );
  }

  const tabs: { key: StructureTab; label: string; count?: number }[] = [
    { key: "columns", label: "Colunas", count: def?.columns.length },
    { key: "indexes", label: "Índices", count: def?.indexes.length },
    { key: "fk", label: "Chaves Est.", count: def?.foreign_keys.length },
    ...(def?.ddl ? [{ key: "ddl" as StructureTab, label: "DDL" }] : []),
  ];

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Panel header */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/10 shrink-0">
        <Table2 className="h-3.5 w-3.5 text-green-400 shrink-0" />
        <span className="text-xs font-medium">{tbl.name}</span>
        <span className="flex-1" />
        {dirtyCount > 0 && (
          <button
            className="px-2 py-0.5 text-xs border border-border rounded hover:bg-accent text-muted-foreground"
            onClick={handleDiscard}
          >
            Descartar
          </button>
        )}
        {activeTab === "columns" && (
          <button
            disabled={dirtyCount === 0}
            className={cn(
              "px-2 py-0.5 text-xs rounded flex items-center gap-1 transition-colors",
              dirtyCount > 0
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-muted text-muted-foreground cursor-not-allowed opacity-50",
            )}
            onClick={handleApply}
          >
            <span className="text-sm leading-none">▶</span>
            Aplicar{dirtyCount > 0 ? ` (${dirtyCount})` : ""}
          </button>
        )}
      </div>

      {/* Subtab strip */}
      <div className="flex items-center border-b border-border shrink-0 bg-muted/5 px-2 pt-1 gap-0.5">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs border-b-2 -mb-px transition-colors whitespace-nowrap",
              activeTab === tab.key
                ? "border-primary text-foreground font-medium"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setActiveTab(tab.key)}
          >
            {tab.label}
            {tab.count !== undefined && (
              <span className={cn(
                "text-[9px] font-semibold px-1 py-0.5 rounded-full leading-none",
                activeTab === tab.key
                  ? "bg-primary/20 text-primary"
                  : "bg-muted text-muted-foreground",
              )}>
                {tab.count}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-auto">

        {/* ── Colunas ── */}
        {activeTab === "columns" && (
        <>
        <div className="overflow-x-auto">
          <table className="w-full text-xs border-collapse min-w-max">
            <thead className="sticky top-0 bg-background z-10">
              <tr className="border-b border-border">
                <th className="px-2 py-1 text-left font-medium text-muted-foreground w-6">#</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[120px]">Nome</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[100px]">Tipo</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[70px]">Comprimento</th>
                <th className="px-2 py-1 text-center font-medium text-muted-foreground w-12">NULL</th>
                <th className="px-2 py-1 text-center font-medium text-muted-foreground w-8">PK</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[100px]">Default</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[120px]">Extra</th>
                <th className="px-2 py-1 text-left font-medium text-muted-foreground min-w-[140px]">Comentário</th>
                <th className="px-2 py-1 w-8" />
              </tr>
            </thead>
            <tbody>
              {cols.map((row, idx) => {
                const dirty = !row.isNew && !row.isDeleted && isDirtyRow(row);
                return (
                  <tr
                    key={row.id}
                    className={cn(
                      "border-b border-border/50 hover:bg-accent/20 transition-colors",
                      row.isDeleted && "opacity-40",
                      row.isNew && "bg-primary/5",
                      dirty && "border-l-2 border-l-amber-500",
                    )}
                  >
                    {/* # */}
                    <td className="px-2 py-0.5 text-muted-foreground text-center select-none">{idx + 1}</td>

                    {/* Name */}
                    <td className={cn("px-2 py-0.5 font-mono font-medium", row.isDeleted && "line-through")}>
                      <EditableCell
                        value={row.name}
                        editing={isEditing(row.id, "name")}
                        onDoubleClick={() => !row.isDeleted && startEdit(row.id, "name")}
                        onCommit={(v) => commitEdit(row.id, "name", v)}
                        onCancel={cancelEdit}
                      />
                    </td>

                    {/* Type */}
                    <td className="px-2 py-0.5">
                      <TypeDropdownCell
                        value={row.base}
                        editing={isEditing(row.id, "base")}
                        onDoubleClick={() => !row.isDeleted && startEdit(row.id, "base")}
                        onCommit={(v) => commitEdit(row.id, "base", v)}
                        onCancel={cancelEdit}
                      />
                    </td>

                    {/* Length */}
                    <td className="px-2 py-0.5">
                      {!TYPES_WITHOUT_LENGTH.has(row.base) ? (
                        <EditableCell
                          value={row.length}
                          editing={isEditing(row.id, "length")}
                          onDoubleClick={() => !row.isDeleted && startEdit(row.id, "length")}
                          onCommit={(v) => commitEdit(row.id, "length", v)}
                          onCancel={cancelEdit}
                          className="text-muted-foreground"
                        />
                      ) : (
                        <span className="text-muted-foreground/40 text-[10px]">—</span>
                      )}
                    </td>

                    {/* NULL toggle */}
                    <td className="px-2 py-0.5 text-center">
                      <button
                        className={cn(
                          "w-4 h-4 rounded border text-[10px] leading-none transition-colors",
                          row.nullable
                            ? "border-primary bg-primary/20 text-primary"
                            : "border-border text-muted-foreground/30",
                        )}
                        onClick={() => !row.isDeleted && setRow(row.id, { nullable: !row.nullable })}
                        title={row.nullable ? "Nullable — click para NOT NULL" : "NOT NULL — click para nullable"}
                      >
                        {row.nullable ? "✓" : ""}
                      </button>
                    </td>

                    {/* PK toggle */}
                    <td className="px-2 py-0.5 text-center">
                      <button
                        className={cn(
                          "text-sm leading-none transition-colors",
                          row.isPrimary ? "text-yellow-500" : "text-muted-foreground/20",
                        )}
                        onClick={() => !row.isDeleted && setRow(row.id, { isPrimary: !row.isPrimary })}
                        title="Primary Key"
                      >
                        ●
                      </button>
                    </td>

                    {/* Default */}
                    <td className="px-2 py-0.5">
                      <EditableCell
                        value={row.defaultVal}
                        editing={isEditing(row.id, "defaultVal")}
                        onDoubleClick={() => !row.isDeleted && startEdit(row.id, "defaultVal")}
                        onCommit={(v) => commitEdit(row.id, "defaultVal", v)}
                        onCancel={cancelEdit}
                        className="text-muted-foreground"
                      >
                        {row.defaultVal === "" ? (
                          <span className="text-muted-foreground/40 text-[10px] italic">—</span>
                        ) : row.defaultVal.toUpperCase() === "NULL" ? (
                          <span className="inline-block px-1 text-[9px] bg-muted rounded text-muted-foreground">NULL</span>
                        ) : (
                          <span className="font-mono">{row.defaultVal}</span>
                        )}
                      </EditableCell>
                    </td>

                    {/* Extra */}
                    <td className="px-2 py-0.5">
                      <EditableCell
                        value={row.extra}
                        editing={isEditing(row.id, "extra")}
                        onDoubleClick={() => !row.isDeleted && startEdit(row.id, "extra")}
                        onCommit={(v) => commitEdit(row.id, "extra", v)}
                        onCancel={cancelEdit}
                      >
                        {row.extra ? (
                          <span className="inline-block px-1.5 text-[9px] bg-cyan-500/20 text-cyan-400 rounded font-mono">
                            {row.extra}
                          </span>
                        ) : (
                          <span className="text-muted-foreground/30 text-[10px]">—</span>
                        )}
                      </EditableCell>
                    </td>

                    {/* Comment */}
                    <td className="px-2 py-0.5">
                      <EditableCell
                        value={row.comment}
                        editing={isEditing(row.id, "comment")}
                        onDoubleClick={() => !row.isDeleted && startEdit(row.id, "comment")}
                        onCommit={(v) => commitEdit(row.id, "comment", v)}
                        onCancel={cancelEdit}
                        className="text-muted-foreground italic"
                      >
                        {row.comment || (
                          <span className="text-muted-foreground/30 text-[10px] not-italic">—</span>
                        )}
                      </EditableCell>
                    </td>

                    {/* Delete */}
                    <td className="px-2 py-0.5 text-center">
                      <button
                        className={cn(
                          "transition-colors",
                          row.isDeleted
                            ? "text-muted-foreground/50 hover:text-foreground"
                            : "text-muted-foreground/30 hover:text-destructive",
                        )}
                        onClick={() =>
                          setRow(row.id, { isDeleted: !row.isDeleted })
                        }
                        title={row.isDeleted ? "Restaurar coluna" : "Marcar para exclusão"}
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>

        {/* Add column */}
        {isMysql && (
          <div className="px-3 py-2 border-t border-border/40">
            <button
              className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
              onClick={handleAddColumn}
            >
              <span className="text-base leading-none">+</span>
              Adicionar coluna
            </button>
          </div>
        )}
        </>
        )}

        {/* ── Índices ── */}

        {activeTab === "indexes" && (
          <div className="p-4">
            {!def || def.indexes.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 gap-2 text-center">
                <span className="text-2xl">🗂</span>
                <p className="text-sm text-muted-foreground">Sem índices definidos</p>
              </div>
            ) : (
              <table className="w-full text-xs border-collapse">
                <thead>
                  <tr className="border-b border-border">
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">Nome</th>
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">Colunas</th>
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium w-16">Único</th>
                  </tr>
                </thead>
                <tbody>
                  {def.indexes.map((idx, i) => (
                    <tr key={i} className="border-b border-border/40 hover:bg-accent/20">
                      <td className="py-1 px-3 font-mono font-medium">{idx.name}</td>
                      <td className="py-1 px-3 text-muted-foreground">{idx.columns.join(", ")}</td>
                      <td className="py-1 px-3 text-center">
                        {idx.unique ? (
                          <span className="inline-block px-1.5 py-0.5 text-[9px] bg-green-500/15 text-green-400 rounded font-semibold">UNIQUE</span>
                        ) : ""}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* ── Chaves Estrangeiras ── */}
        {activeTab === "fk" && (
          <div className="p-4">
            {!def || def.foreign_keys.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 gap-2 text-center">
                <span className="text-2xl">🔗</span>
                <p className="text-sm text-muted-foreground">Sem chaves estrangeiras</p>
              </div>
            ) : (
              <table className="w-full text-xs border-collapse">
                <thead>
                  <tr className="border-b border-border">
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">Constraint</th>
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">Coluna</th>
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">→ Tabela</th>
                    <th className="text-left py-1.5 px-3 text-muted-foreground font-medium">→ Coluna</th>
                  </tr>
                </thead>
                <tbody>
                  {def.foreign_keys.map((fk, i) => (
                    <tr key={i} className="border-b border-border/40 hover:bg-accent/20">
                      <td className="py-1 px-3 font-mono text-xs font-medium">{fk.name}</td>
                      <td className="py-1 px-3 font-mono text-blue-400">{fk.columns.join(", ")}</td>
                      <td className="py-1 px-3 font-mono text-yellow-400">{fk.ref_table}</td>
                      <td className="py-1 px-3 font-mono text-muted-foreground">{fk.ref_columns.join(", ")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* ── DDL ── */}
        {activeTab === "ddl" && (
          <div className="p-4">
            {def?.ddl ? (
              <pre className="text-xs font-mono text-green-400 bg-black/30 rounded-lg p-4 whitespace-pre-wrap overflow-x-auto border border-border/40">
                {def.ddl}
              </pre>
            ) : (
              <div className="flex flex-col items-center justify-center py-12 gap-2 text-center">
                <p className="text-sm text-muted-foreground">DDL não disponível</p>
              </div>
            )}
          </div>
        )}

      </div>

      {/* SQL preview modal */}
      {previewSql && (
        <SqlPreviewModal
          sql={previewSql}
          onCancel={() => setPreviewSql(null)}
          onExecute={() => void handleExecute()}
          executing={executing}
        />
      )}
    </div>
  );
}
