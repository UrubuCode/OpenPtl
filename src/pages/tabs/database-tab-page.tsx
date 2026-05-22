import {
  ChevronDown,
  ChevronRight,
  ClipboardCopy,
  ClipboardPaste,
  Database,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Table2,
  Terminal,
  Unplug,
  Users,
  X,
} from "lucide-react";
import Editor from "@monaco-editor/react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";

import { useT } from "@/langs";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/app-store";
import { UserManagerModal } from "@/components/drawers/database-user-manager-modal";
import { ColumnStructurePanel } from "@/pages/tabs/database-column-panel";
import type {
  DbQueryResult,
  DbTableDefinition,
  DbTableInfo,
} from "@/types/openptl";

type MainTab = "database" | "table" | "data" | { kind: "query"; id: string };
type ConnectStage = "connecting" | "ready" | "error";

interface QueryTab {
  id: string;
  label: string;
  text: string;
  result: DbQueryResult | null;
  running: boolean;
  error: string | null;
}

interface ContextMenu {
  x: number;
  y: number;
  cell: string;
  row: unknown[];
  colNames: string[];
}

interface ConsoleLine {
  sql: string;
  ms?: number;
  error?: string;
  ts: number;
}

interface EditingCell {
  rowIdx: number;
  colIdx: number;
  colName: string;
  original: unknown;
  value: string;
}

interface DatabaseTabPageProps {
  tabId: string;
  profileId: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatBytes(bytes: number | null): string {
  if (bytes === null || bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

let _queryTabSeq = 0;
function newQueryTab(label: string, text = ""): QueryTab {
  return { id: `qt-${++_queryTabSeq}`, label, text, result: null, running: false, error: null };
}

// ─── Context menu ─────────────────────────────────────────────────────────────

function TableContextMenu({ menu, onClose }: { menu: ContextMenu; onClose: () => void }) {
  const cellStr = menu.cell === null ? "NULL" : String(menu.cell);
  const rowStr = menu.colNames
    .map((c, i) => `${c}: ${menu.row[i] === null ? "NULL" : String(menu.row[i])}`)
    .join("\n");
  const allCols = menu.colNames.join("\t");

  const action = (fn: () => void) => () => { fn(); onClose(); };

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        className="fixed z-50 min-w-[180px] rounded-md border border-border bg-popover shadow-xl py-1 text-xs"
        style={{ left: menu.x, top: menu.y }}
      >
        <button className="w-full text-left px-3 py-1.5 hover:bg-accent" onClick={action(() => void navigator.clipboard.writeText(cellStr))}>
          Copiar célula
        </button>
        <button className="w-full text-left px-3 py-1.5 hover:bg-accent" onClick={action(() => void navigator.clipboard.writeText(rowStr))}>
          Copiar linha
        </button>
        <button className="w-full text-left px-3 py-1.5 hover:bg-accent" onClick={action(() => void navigator.clipboard.writeText(allCols))}>
          Copiar cabeçalhos
        </button>
        <div className="border-t border-border my-1" />
        <button className="w-full text-left px-3 py-1.5 hover:bg-accent text-muted-foreground" onClick={onClose}>
          Fechar
        </button>
      </div>
    </>
  );
}

// ─── Error modal ──────────────────────────────────────────────────────────────

function ErrorModal({ title, message, onClose }: { title: string; message: string; onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div className="bg-popover border border-border rounded-lg shadow-xl p-4 max-w-lg w-full mx-4" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-2">
          <span className="text-sm font-medium text-destructive">{title}</span>
          <button onClick={onClose}><X className="h-4 w-4 text-muted-foreground" /></button>
        </div>
        <pre className="text-xs font-mono text-destructive whitespace-pre-wrap bg-destructive/10 rounded p-2 max-h-60 overflow-auto">
          {message}
        </pre>
        <button className="mt-3 px-3 py-1 text-xs border border-border rounded hover:bg-accent w-full" onClick={onClose}>
          Fechar
        </button>
      </div>
    </div>
  );
}

// ─── Results grid ─────────────────────────────────────────────────────────────

function ResultsGrid({
  result,
  tableDef,
  database,
  schema,
  table,
  sessionId,
  isMysql,
  onContextMenu,
  onCellUpdated,
}: {
  result: DbQueryResult;
  tableDef?: DbTableDefinition | null;
  database?: string;
  schema?: string;
  table?: string;
  sessionId?: string | null;
  isMysql?: boolean;
  onContextMenu?: (menu: ContextMenu) => void;
  onCellUpdated?: () => void;
}) {
  const t = useT();
  const [filterText, setFilterText] = useState("");
  const [showFilter, setShowFilter] = useState(false);
  const [editingCell, setEditingCell] = useState<EditingCell | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const filterRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Ctrl+F to toggle filter
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "f") {
        if (!containerRef.current?.contains(document.activeElement) && document.activeElement?.closest(".monaco-editor")) return;
        e.preventDefault();
        setShowFilter((v) => {
          if (!v) setTimeout(() => filterRef.current?.focus(), 50);
          return !v;
        });
      }
      if (e.key === "Escape") setShowFilter(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const colNames = result.columns.map((c) => c.name);

  const filteredRows = filterText
    ? result.rows.filter((row) =>
        row.some((cell) =>
          String(cell ?? "").toLowerCase().includes(filterText.toLowerCase()),
        ),
      )
    : result.rows;

  const handleCellBlur = async () => {
    if (!editingCell) return;
    const original = editingCell.original;
    const newVal = editingCell.value;

    if (newVal === String(original ?? "")) {
      setEditingCell(null);
      return;
    }

    const pkCols = tableDef?.primary_key ?? [];
    if (!pkCols.length || !sessionId || !database || !table) {
      setEditingCell(null);
      return;
    }

    const row = result.rows[editingCell.rowIdx];
    const escape = (v: string) => v.replace(/\\/g, "\\\\").replace(/'/g, "\\'");

    const where = pkCols
      .map((pk) => {
        const pkIdx = colNames.indexOf(pk);
        const val = row[pkIdx];
        if (val === null || val === undefined) return `\`${pk}\` IS NULL`;
        return `\`${pk}\` = '${escape(String(val))}'`;
      })
      .join(" AND ");

    const setExpr =
      newVal === "" || newVal.toUpperCase() === "NULL"
        ? `\`${editingCell.colName}\` = NULL`
        : `\`${editingCell.colName}\` = '${escape(newVal)}'`;

    const tblPath = isMysql
      ? `\`${database}\`.\`${table}\``
      : `"${schema}"."${table}"`;

    const sql = `UPDATE ${tblPath} SET ${setExpr} WHERE ${where}`;

    try {
      await api.dbQuery(sessionId, sql);
      setEditingCell(null);
      onCellUpdated?.();
    } catch (err) {
      setEditError(err instanceof Error ? err.message : String(err));
      setEditingCell(null);
    }
  };

  if (result.rows.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        {t.database.noResults}
        {result.affected_rows > 0 && (
          <span className="ml-2">({result.affected_rows} {t.database.affectedRows})</span>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden" ref={containerRef}>
      {editError && <ErrorModal title="Erro ao atualizar" message={editError} onClose={() => setEditError(null)} />}

      {/* Filter bar */}
      {showFilter && (
        <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-muted/20 shrink-0">
          <Search className="h-3 w-3 text-muted-foreground shrink-0" />
          <input
            ref={filterRef}
            className="flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground"
            placeholder={t.database.filterPlaceholder}
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            onKeyDown={(e: ReactKeyboardEvent) => { if (e.key === "Escape") { setShowFilter(false); setFilterText(""); } }}
          />
          {filterText && (
            <span className="text-[10px] text-muted-foreground">{filteredRows.length}/{result.rows.length}</span>
          )}
          <button onClick={() => { setShowFilter(false); setFilterText(""); }}>
            <X className="h-3 w-3 text-muted-foreground hover:text-foreground" />
          </button>
        </div>
      )}

      <div className="overflow-auto flex-1">
        <table className="w-full text-xs border-collapse min-w-max">
          <thead className="sticky top-0 bg-background z-10">
            <tr>
              {result.columns.map((col, i) => (
                <th
                  key={i}
                  className="px-2 py-1 text-left border-b border-border font-medium text-muted-foreground whitespace-nowrap"
                >
                  {col.name}
                  <span className="ml-1 text-[10px] opacity-60">{col.type_name}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {filteredRows.map((row, ri) => (
              <tr key={ri} className="hover:bg-accent/30 border-b border-border/50">
                {row.map((cell, ci) => {
                  const isEditing = editingCell?.rowIdx === ri && editingCell?.colIdx === ci;
                  return (
                    <td
                      key={ci}
                      className="px-2 py-0.5 align-top cursor-default"
                      onDoubleClick={() => {
                        if (!table || !tableDef?.primary_key?.length) return;
                        setEditingCell({
                          rowIdx: ri,
                          colIdx: ci,
                          colName: colNames[ci],
                          original: cell,
                          value: cell === null || cell === undefined ? "" : String(cell),
                        });
                      }}
                      onContextMenu={(e: ReactMouseEvent) => {
                        e.preventDefault();
                        onContextMenu?.({ x: e.clientX, y: e.clientY, cell: cell as string, row, colNames });
                      }}
                    >
                      {isEditing ? (
                        <input
                          autoFocus
                          className="w-full bg-primary/10 border border-primary rounded px-1 text-xs font-mono outline-none min-w-[60px]"
                          value={editingCell!.value}
                          onChange={(e) =>
                            setEditingCell((prev) => prev ? { ...prev, value: e.target.value } : null)
                          }
                          onBlur={() => void handleCellBlur()}
                          onKeyDown={(e: ReactKeyboardEvent<HTMLInputElement>) => {
                            if (e.key === "Enter") { e.currentTarget.blur(); }
                            if (e.key === "Escape") setEditingCell(null);
                          }}
                        />
                      ) : cell === null || cell === undefined ? (
                        <span className="text-muted-foreground italic">NULL</span>
                      ) : typeof cell === "boolean" ? (
                        <span className={cn("font-mono", cell ? "text-green-500" : "text-red-500")}>{String(cell)}</span>
                      ) : typeof cell === "number" ? (
                        <span className="font-mono text-blue-400 text-right block">{String(cell)}</span>
                      ) : (
                        <span className="font-mono truncate block max-w-[200px]" title={String(cell)}>{String(cell)}</span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── Object tree ─────────────────────────────────────────────────────────────

const ALL_DB_COLS = [
  "colRows", "colSize", "colEngine", "colComment", "colVersion",
  "colRowFormat", "colAvgRowLen", "colMaxDataLen", "colIndexLen",
  "colDataFree", "colAutoIncrement", "colCheckTime", "colCollation",
  "colChecksum", "colCreateOptions", "colType", "colCreatedAt", "colUpdatedAt",
] as const;
type DbColKey = (typeof ALL_DB_COLS)[number];

function ObjectTree({
  tables,
  databases,
  expandedDbs,
  loadingNode,
  selectedDatabase,
  selectedTable,
  isMysql,
  onToggleDb,
  onSelectDatabase,
  onSelectTable,
}: {
  tables: Record<string, DbTableInfo[]>;
  databases: string[];
  expandedDbs: Set<string>;
  loadingNode: string | null;
  selectedDatabase: string | null;
  selectedTable: string | null;
  isMysql: boolean;
  onToggleDb: (db: string) => void;
  onSelectDatabase: (db: string) => void;
  onSelectTable: (db: string, tbl: DbTableInfo) => void;
}) {
  const t = useT();
  return (
    <div className="w-52 shrink-0 border-r border-border overflow-auto bg-sidebar text-sidebar-foreground flex flex-col select-none">
      <div className="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-wide border-b border-border shrink-0">
        {t.database.databases}
      </div>
      <div className="flex-1 overflow-auto text-xs">
        {loadingNode === "root" && (
          <div className="px-3 py-2 text-muted-foreground">{t.database.loadingTree}</div>
        )}
        {databases.map((db) => {
          const dbExpanded = expandedDbs.has(db);
          const dbTables = tables[db] ?? [];
          return (
            <div key={db}>
              <div
                className={cn(
                  "flex items-center gap-1 px-1 py-0.5 cursor-pointer hover:bg-accent/50",
                  selectedDatabase === db && !selectedTable && "bg-accent",
                )}
                onClick={() => onSelectDatabase(db)}
              >
                <button
                  className="shrink-0 p-0.5 text-muted-foreground hover:text-foreground"
                  onClick={(e) => { e.stopPropagation(); onToggleDb(db); }}
                >
                  {dbExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                </button>
                <Database className="h-3.5 w-3.5 shrink-0 text-yellow-500" />
                <span className="truncate">{db}</span>
              </div>
              {dbExpanded && (
                <>
                  {loadingNode === `db:${db}` && (
                    <div className="pl-8 py-0.5 text-muted-foreground">{t.database.loadingTree}</div>
                  )}
                  {dbTables.map((tbl) => (
                    <div
                      key={tbl.name}
                      className={cn(
                        "flex items-center gap-1 pl-7 pr-1 py-0.5 cursor-pointer hover:bg-accent/50",
                        selectedTable === tbl.name && selectedDatabase === db && "bg-accent",
                      )}
                      onClick={() => onSelectTable(db, tbl)}
                    >
                      <Table2 className={cn("h-3.5 w-3.5 shrink-0", tbl.kind === "view" ? "text-blue-400" : "text-green-400")} />
                      <span className="truncate flex-1">{tbl.name}</span>
                      {tbl.size_bytes !== null && isMysql && (
                        <span className="text-[9px] text-muted-foreground shrink-0">{formatBytes(tbl.size_bytes)}</span>
                      )}
                    </div>
                  ))}
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─── Database info panel ──────────────────────────────────────────────────────

function DatabasePanel({
  dbName,
  tables,
  onSelectTable,
}: {
  dbName: string;
  tables: DbTableInfo[];
  onSelectTable: (tbl: DbTableInfo) => void;
}) {
  const t = useT();
  const [visibleCols, setVisibleCols] = useState<Set<DbColKey>>(
    new Set(["colRows", "colSize", "colEngine", "colComment", "colCreatedAt", "colUpdatedAt"]),
  );
  const [showColPicker, setShowColPicker] = useState(false);

  const toggleCol = (key: DbColKey) => {
    setVisibleCols((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key); else next.add(key);
      return next;
    });
  };

  const getCellValue = (tbl: DbTableInfo, key: DbColKey): string => {
    switch (key) {
      case "colRows": return tbl.rows !== null ? String(tbl.rows) : "—";
      case "colSize": return formatBytes(tbl.size_bytes);
      case "colEngine": return tbl.engine ?? "—";
      case "colComment": return tbl.comment ?? "";
      case "colVersion": return tbl.version !== null ? String(tbl.version) : "—";
      case "colRowFormat": return tbl.row_format ?? "—";
      case "colAvgRowLen": return tbl.avg_row_length !== null ? formatBytes(tbl.avg_row_length) : "—";
      case "colMaxDataLen": return tbl.max_data_length !== null ? formatBytes(tbl.max_data_length) : "—";
      case "colIndexLen": return tbl.index_length !== null ? formatBytes(tbl.index_length) : "—";
      case "colDataFree": return tbl.data_free !== null ? formatBytes(tbl.data_free) : "—";
      case "colAutoIncrement": return tbl.auto_increment !== null ? String(tbl.auto_increment) : "—";
      case "colCheckTime": return tbl.check_time ?? "—";
      case "colCollation": return tbl.collation ?? "—";
      case "colChecksum": return tbl.checksum !== null ? String(tbl.checksum) : "—";
      case "colCreateOptions": return tbl.create_options ?? "";
      case "colType": return tbl.kind;
      case "colCreatedAt": return tbl.created_at ?? "—";
      case "colUpdatedAt": return tbl.updated_at ?? "—";
    }
  };

  const activeCols = ALL_DB_COLS.filter((k) => visibleCols.has(k));

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border bg-muted/10 shrink-0">
        <Database className="h-3.5 w-3.5 text-yellow-500" />
        <span className="text-xs font-medium">{dbName}</span>
        <span className="text-[10px] text-muted-foreground">{tables.length} {t.database.tables}</span>
        <span className="flex-1" />
        <div className="relative">
          <button
            className="flex items-center gap-1 px-2 py-0.5 text-xs border border-border rounded hover:bg-accent text-muted-foreground"
            onClick={() => setShowColPicker((v) => !v)}
          >
            <Settings2 className="h-3 w-3" />
            {t.database.toggleColumns}
          </button>
          {showColPicker && (
            <div className="absolute right-0 top-7 z-30 bg-popover border border-border rounded shadow-xl p-2 min-w-[200px] grid grid-cols-2 gap-0.5">
              {ALL_DB_COLS.map((key) => (
                <label key={key} className="flex items-center gap-1.5 text-xs cursor-pointer px-1 py-0.5 hover:bg-accent rounded">
                  <input type="checkbox" checked={visibleCols.has(key)} onChange={() => toggleCol(key)} className="accent-primary" />
                  {t.database[key]}
                </label>
              ))}
            </div>
          )}
        </div>
      </div>
      <div className="flex-1 overflow-auto">
        <table className="w-full text-xs border-collapse min-w-max">
          <thead className="sticky top-0 bg-background z-10">
            <tr>
              <th className="px-2 py-1 text-left border-b border-border font-medium text-muted-foreground whitespace-nowrap">
                Nome
              </th>
              {activeCols.map((key) => (
                <th key={key} className="px-2 py-1 text-left border-b border-border font-medium text-muted-foreground whitespace-nowrap">
                  {t.database[key]}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {tables.map((tbl, i) => (
              <tr
                key={i}
                className="hover:bg-accent/40 border-b border-border/50 cursor-pointer"
                onClick={() => onSelectTable(tbl)}
              >
                <td className="px-2 py-0.5 font-mono font-medium whitespace-nowrap">
                  <span className="flex items-center gap-1">
                    <Table2 className={cn("h-3 w-3 shrink-0", tbl.kind === "view" ? "text-blue-400" : "text-green-400")} />
                    {tbl.name}
                  </span>
                </td>
                {activeCols.map((key) => (
                  <td key={key} className="px-2 py-0.5 text-muted-foreground whitespace-nowrap">
                    {getCellValue(tbl, key)}
                  </td>
                ))}
              </tr>
            ))}
            {tables.length === 0 && (
              <tr>
                <td colSpan={activeCols.length + 1} className="px-2 py-4 text-center text-muted-foreground">
                  {t.database.emptyDatabase}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── Console panel ────────────────────────────────────────────────────────────

function ConsolePanel({ lines }: { lines: ConsoleLine[] }) {
  const bottomRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  return (
    <div className="flex flex-col h-full overflow-hidden bg-black/50">
      <div className="flex items-center gap-2 px-3 py-1 border-b border-border shrink-0 text-[10px] text-muted-foreground">
        <Terminal className="h-3 w-3" />
        Console SQL
      </div>
      <div className="flex-1 overflow-auto px-3 py-1 font-mono text-[11px] space-y-0.5">
        {lines.map((line, i) => (
          <div key={i} className={cn("leading-5", line.error ? "text-red-400" : "text-green-400/90")}>
            <span className="text-muted-foreground/60 select-none mr-2">{new Date(line.ts).toLocaleTimeString()}</span>
            {line.error ? (
              <>
                <span className="text-muted-foreground">{line.sql}</span>
                <span className="ml-2 text-red-400">— {line.error}</span>
              </>
            ) : (
              <>
                {line.sql}
                {line.ms !== undefined && <span className="text-muted-foreground/60 ml-2">({line.ms}ms)</span>}
              </>
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function DatabaseTabPage({ tabId: _tabId, profileId }: DatabaseTabPageProps) {
  const t = useT();
  const profile = useAppStore((s) => s.databaseProfiles.find((p) => p.id === profileId) ?? null);
  const closeTab = useAppStore((s) => s.closeTab);

  const [connectStage, setConnectStage] = useState<ConnectStage>("connecting");
  const [connectError, setConnectError] = useState<string | null>(null);

  const isMysql = profile?.driver === "mysql" || profile?.driver === "mariadb";
  const supportsUserMgmt = isMysql || profile?.driver === "postgres";

  const [databases, setDatabases] = useState<string[]>([]);
  const [tables, setTables] = useState<Record<string, DbTableInfo[]>>({});
  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [loadingNode, setLoadingNode] = useState<string | null>(null);
  const [selectedDatabase, setSelectedDatabase] = useState<string | null>(null);
  const [selectedTableInfo, setSelectedTableInfo] = useState<DbTableInfo | null>(null);

  const [mainTab, setMainTab] = useState<MainTab>({ kind: "query", id: "" });

  const [tableDef, setTableDef] = useState<DbTableDefinition | null>(null);
  const [tableData, setTableData] = useState<DbQueryResult | null>(null);
  const [tableDataLoading, setTableDataLoading] = useState(false);

  // Multiple query tabs — activeQueryTabId initialized to "" to avoid reading
  // queryTabs[0].id which can be undefined during React Fast Refresh.
  const [queryTabs, setQueryTabs] = useState<QueryTab[]>(() => [
    newQueryTab(`${t.database.queryTab} #1`),
  ]);
  const [activeQueryTabId, setActiveQueryTabId] = useState<string>("");

  const [consoleLogs, setConsoleLogs] = useState<ConsoleLine[]>([]);
  const [showConsole, setShowConsole] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);
  const [showUserManager, setShowUserManager] = useState(false);
  const [errorModal, setErrorModal] = useState<string | null>(null);

  const queryEditorRef = useRef<unknown>(null);
  const sessionIdRef = useRef<string | null>(null);
  const mountedRef = useRef(true);

  // Sync activeQueryTabId and mainTab once queryTabs is initialized (id starts as "")
  useEffect(() => {
    const firstId = queryTabs[0]?.id;
    if (!firstId) return;
    if (!activeQueryTabId) {
      setActiveQueryTabId(firstId);
    }
    if (typeof mainTab === "object" && mainTab.kind === "query" && mainTab.id === "") {
      setMainTab({ kind: "query", id: firstId });
    }
  }, [queryTabs, mainTab, activeQueryTabId]);

  const pushConsole = useCallback((sql: string, ms?: number, error?: string) => {
    setConsoleLogs((prev) => [...prev.slice(-200), { sql, ms, error, ts: Date.now() }]);
    if (error) setShowConsole(true);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    if (!profile) { setConnectStage("error"); setConnectError("Profile not found"); return; }
    void (async () => {
      try {
        const sid = await api.dbConnect(profile.id);
        if (!mountedRef.current) { void api.dbDisconnect(sid); return; }
        sessionIdRef.current = sid;
        setConnectStage("ready");
        setLoadingNode("root");
        const dbs = await api.dbListDatabases(sid);
        if (mountedRef.current) { setDatabases(dbs); setLoadingNode(null); }
      } catch (err) {
        if (mountedRef.current) {
          setConnectStage("error");
          setConnectError(err instanceof Error ? err.message : String(err));
        }
      }
    })();
    return () => {
      mountedRef.current = false;
      if (sessionIdRef.current) { void api.dbDisconnect(sessionIdRef.current); sessionIdRef.current = null; }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId]);

  const retry = useCallback(async () => {
    if (!profile) return;
    setConnectStage("connecting"); setConnectError(null); setDatabases([]); setTables({});
    try {
      const sid = await api.dbConnect(profile.id);
      sessionIdRef.current = sid;
      setConnectStage("ready");
      setLoadingNode("root");
      const dbs = await api.dbListDatabases(sid);
      setDatabases(dbs); setLoadingNode(null);
    } catch (err) {
      setConnectStage("error");
      setConnectError(err instanceof Error ? err.message : String(err));
    }
  }, [profile]);

  const loadTablesForDb = useCallback(async (db: string) => {
    const sid = sessionIdRef.current;
    if (!sid || tables[db]) return;
    setLoadingNode(`db:${db}`);
    const schema = isMysql ? db : "public";
    const tbls = await api.dbListTables(sid, db, schema).catch(() => [] as DbTableInfo[]);
    setTables((prev) => ({ ...prev, [db]: tbls }));
    setLoadingNode(null);
  }, [tables, isMysql]);

  const handleToggleDb = useCallback((db: string) => {
    setExpandedDbs((prev) => {
      const next = new Set(prev);
      if (next.has(db)) next.delete(db);
      else { next.add(db); void loadTablesForDb(db); }
      return next;
    });
  }, [loadTablesForDb]);

  const handleSelectDatabase = useCallback(async (db: string) => {
    setSelectedDatabase(db);
    setSelectedTableInfo(null);
    setTableDef(null);
    setTableData(null);
    setMainTab("database");
    const sid = sessionIdRef.current;
    if (sid && isMysql) await api.dbSetDatabase(sid, db).catch(() => null);
    void loadTablesForDb(db);
  }, [loadTablesForDb, isMysql]);

  const handleSelectTable = useCallback(async (db: string, tbl: DbTableInfo) => {
    const sid = sessionIdRef.current;
    setSelectedDatabase(db);
    setSelectedTableInfo(tbl);
    setTableDef(null);
    setTableData(null);
    setMainTab("table");
    if (!sid) return;
    if (isMysql) await api.dbSetDatabase(sid, db).catch(() => null);
    const schema = isMysql ? db : "public";
    const descSql = isMysql ? `SHOW CREATE TABLE \`${db}\`.\`${tbl.name}\`` : `SELECT * FROM information_schema.columns WHERE table_schema='${schema}' AND table_name='${tbl.name}'`;
    pushConsole(descSql);
    const def = await api.dbDescribeTable(sid, db, schema, tbl.name).catch((e: unknown) => {
      pushConsole(descSql, undefined, e instanceof Error ? e.message : String(e));
      return null;
    });
    setTableDef(def);
  }, [isMysql, pushConsole]);

  const handleSelectTableFromDbPanel = useCallback(async (tbl: DbTableInfo) => {
    if (!selectedDatabase) return;
    await handleSelectTable(selectedDatabase, tbl);
  }, [selectedDatabase, handleSelectTable]);

  const loadTableData = useCallback(async (limit = 500, offset = 0) => {
    const sid = sessionIdRef.current;
    if (!sid || !selectedDatabase || !selectedTableInfo) return;
    setTableDataLoading(true);
    setTableData(null);
    const schema = isMysql ? selectedDatabase : "public";
    const sql = isMysql
      ? `SELECT * FROM \`${selectedDatabase}\`.\`${selectedTableInfo.name}\` LIMIT ${limit} OFFSET ${offset}`
      : `SELECT * FROM "${schema}"."${selectedTableInfo.name}" LIMIT ${limit} OFFSET ${offset}`;
    pushConsole(sql);
    try {
      const result = await api.dbGetTableData(sid, selectedDatabase, schema, selectedTableInfo.name, limit, offset);
      setTableData(result);
      pushConsole(sql, result.duration_ms);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      pushConsole(sql, undefined, msg);
      setErrorModal(msg);
    } finally {
      setTableDataLoading(false);
    }
  }, [selectedDatabase, selectedTableInfo, isMysql, pushConsole]);

  // Auto-load data when switching to data tab
  useEffect(() => {
    if (mainTab === "data" && selectedTableInfo && !tableData && !tableDataLoading) {
      void loadTableData();
    }
  }, [mainTab, selectedTableInfo, tableData, tableDataLoading, loadTableData]);

  const updateQueryTab = useCallback((id: string, update: Partial<QueryTab>) => {
    setQueryTabs((prev) => prev.map((qt) => qt.id === id ? { ...qt, ...update } : qt));
  }, []);

  const runQuery = useCallback(async () => {
    const sid = sessionIdRef.current;
    if (!sid) return;
    const activeTab = queryTabs.find((qt) => qt.id === activeQueryTabId);
    if (!activeTab || activeTab.running || !activeTab.text.trim()) return;

    updateQueryTab(activeQueryTabId, { running: true, error: null, result: null });
    pushConsole(activeTab.text.trim());

    try {
      const result = await api.dbQuery(sid, activeTab.text);
      updateQueryTab(activeQueryTabId, { running: false, result, error: null });
      pushConsole(activeTab.text.trim(), result.duration_ms);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      updateQueryTab(activeQueryTabId, { running: false, error: msg });
      pushConsole(activeTab.text.trim(), undefined, msg);
      setErrorModal(msg);
    }
  }, [queryTabs, activeQueryTabId, updateQueryTab, pushConsole]);

  const addQueryTab = useCallback(() => {
    const n = queryTabs.length + 1;
    const tab = newQueryTab(`${t.database.queryTab} #${n}`);
    setQueryTabs((prev) => [...prev, tab]);
    setActiveQueryTabId(tab.id);
    setMainTab({ kind: "query", id: tab.id });
  }, [queryTabs.length, t.database.queryTab]);

  const removeQueryTab = useCallback((id: string) => {
    setQueryTabs((prev) => {
      if (prev.length === 1) return prev;
      const next = prev.filter((qt) => qt.id !== id);
      if (activeQueryTabId === id) {
        const newActive = next[next.length - 1].id;
        setActiveQueryTabId(newActive);
        setMainTab({ kind: "query", id: newActive });
      }
      return next;
    });
  }, [activeQueryTabId]);

  // Toolbar actions
  const handleToolbarRefresh = useCallback(() => {
    if (mainTab === "database" && selectedDatabase) {
      setTables((prev) => { const next = { ...prev }; delete next[selectedDatabase]; return next; });
      void loadTablesForDb(selectedDatabase);
    } else if (mainTab === "table" && selectedTableInfo && selectedDatabase) {
      void handleSelectTable(selectedDatabase, selectedTableInfo);
    } else if (mainTab === "data") {
      void loadTableData();
    }
  }, [mainTab, selectedDatabase, selectedTableInfo, loadTablesForDb, handleSelectTable, loadTableData]);

  const handleManageUsers = useCallback(() => {
    setShowUserManager(true);
  }, []);

  const activeQueryTab = queryTabs.find((qt) => qt.id === activeQueryTabId) ?? queryTabs[0];

  if (connectStage === "connecting") {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground gap-2">
        <RefreshCw className="h-4 w-4 animate-spin" />
        {t.database.connecting}
      </div>
    );
  }

  if (connectStage === "error") {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3">
        <Database className="h-8 w-8 text-destructive" />
        <p className="text-sm text-destructive max-w-sm text-center">{connectError ?? t.database.errorConnecting}</p>
        <div className="flex gap-2">
          <button className="px-3 py-1.5 text-sm border border-border rounded hover:bg-accent flex items-center gap-1" onClick={() => void retry()}>
            <RefreshCw className="h-3 w-3" /> {t.database.retry}
          </button>
          <button className="px-3 py-1.5 text-sm border border-border rounded hover:bg-accent" onClick={() => closeTab(_tabId)}>
            {t.database.close}
          </button>
        </div>
      </div>
    );
  }

  const isQueryTab = typeof mainTab === "object" && mainTab.kind === "query";

  return (
    <div className="flex flex-col h-full overflow-hidden bg-background" onClick={() => contextMenu && setContextMenu(null)}>
      {/* Toolbar */}
      <div className="flex items-center gap-1 border-b border-border px-2 py-1 shrink-0 bg-sidebar text-sidebar-foreground text-xs">
        <Database className="h-4 w-4 text-yellow-500 shrink-0" />
        <span className="font-medium truncate max-w-[120px]">{profile?.name ?? "Database"}</span>
        <span className="font-mono px-1.5 py-0.5 rounded bg-muted text-muted-foreground uppercase text-[10px]">
          {profile?.driver}
        </span>

        <div className="w-px h-4 bg-border mx-1 shrink-0" />

        <button className="px-1.5 py-0.5 rounded hover:bg-accent text-muted-foreground" title={t.database.toolbarCopy} onClick={() => void navigator.clipboard.readText().catch(() => null)}>
          <ClipboardCopy className="h-3.5 w-3.5" />
        </button>
        <button
          className="px-1.5 py-0.5 rounded hover:bg-accent text-muted-foreground"
          title={t.database.toolbarPaste}
          onClick={async () => {
            const text = await navigator.clipboard.readText().catch(() => "");
            if (!text) return;
            updateQueryTab(activeQueryTabId, { text: (activeQueryTab?.text ?? "") + text });
          }}
        >
          <ClipboardPaste className="h-3.5 w-3.5" />
        </button>

        <div className="w-px h-4 bg-border mx-1 shrink-0" />

        <button
          className="px-1.5 py-0.5 rounded hover:bg-accent text-muted-foreground"
          title={t.database.toolbarRefresh}
          onClick={handleToolbarRefresh}
        >
          <RefreshCw className="h-3.5 w-3.5" />
        </button>

        {supportsUserMgmt && (
          <button
            className="px-1.5 py-0.5 rounded hover:bg-accent text-muted-foreground"
            title={t.database.manageUsers}
            onClick={handleManageUsers}
          >
            <Users className="h-3.5 w-3.5" />
          </button>
        )}

        <div className="w-px h-4 bg-border mx-1 shrink-0" />

        <button
          disabled={!isQueryTab || activeQueryTab?.running || !activeQueryTab?.text?.trim()}
          className="px-2 py-0.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          title="Executar (Ctrl+Enter)"
          onClick={() => void runQuery()}
        >
          {activeQueryTab?.running
            ? <RefreshCw className="h-3.5 w-3.5 animate-spin" />
            : <span className="text-sm leading-none">▶</span>}
        </button>

        <span className="flex-1" />

        <button
          className={cn("flex items-center gap-1 px-1.5 py-0.5 rounded", showConsole ? "text-foreground bg-accent" : "text-muted-foreground hover:bg-accent/50")}
          onClick={() => setShowConsole((v) => !v)}
          title="Console SQL"
        >
          <Terminal className="h-3.5 w-3.5" />
        </button>
        <button
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-muted-foreground hover:text-destructive"
          title={t.database.disconnect}
          onClick={() => closeTab(_tabId)}
        >
          <Unplug className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* Body */}
      <div className="flex flex-1 overflow-hidden">
        {/* Tree */}
        <ObjectTree
          databases={databases}
          tables={tables}
          expandedDbs={expandedDbs}
          loadingNode={loadingNode}
          selectedDatabase={selectedDatabase}
          selectedTable={selectedTableInfo?.name ?? null}
          isMysql={isMysql}
          onToggleDb={handleToggleDb}
          onSelectDatabase={(db) => void handleSelectDatabase(db)}
          onSelectTable={(db, tbl) => void handleSelectTable(db, tbl)}
        />

        {/* Main + console */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Tab strip */}
          <div className="flex items-center border-b border-border px-2 pt-1 shrink-0 bg-muted/5 overflow-x-auto">
            {selectedDatabase && (
              <button
                className={cn(
                  "px-3 py-1 text-xs border-b-2 -mb-px transition-colors max-w-[180px] truncate shrink-0",
                  mainTab === "database" ? "border-primary text-foreground font-medium" : "border-transparent text-muted-foreground hover:text-foreground",
                )}
                onClick={() => setMainTab("database")}
                title={`${t.database.tabDatabase} — ${selectedDatabase}`}
              >
                {t.database.tabDatabase} — {selectedDatabase}
              </button>
            )}
            {selectedTableInfo && (
              <button
                className={cn(
                  "px-3 py-1 text-xs border-b-2 -mb-px transition-colors max-w-[180px] truncate shrink-0",
                  mainTab === "table" ? "border-primary text-foreground font-medium" : "border-transparent text-muted-foreground hover:text-foreground",
                )}
                onClick={() => setMainTab("table")}
                title={`${t.database.tabTable}: ${selectedTableInfo.name}`}
              >
                {t.database.tabTable}: {selectedTableInfo.name}
              </button>
            )}
            {selectedTableInfo && (
              <button
                className={cn(
                  "px-3 py-1 text-xs border-b-2 -mb-px transition-colors max-w-[180px] truncate shrink-0",
                  mainTab === "data" ? "border-primary text-foreground font-medium" : "border-transparent text-muted-foreground hover:text-foreground",
                )}
                onClick={() => setMainTab("data")}
                title={`${t.database.tabData}: ${selectedTableInfo.name}`}
              >
                {t.database.tabData}: {selectedTableInfo.name}
              </button>
            )}

            {/* Query tabs */}
            {queryTabs.map((qt) => (
              <div
                key={qt.id}
                className={cn(
                  "flex items-center gap-1 px-2 py-1 text-xs border-b-2 -mb-px transition-colors cursor-pointer shrink-0",
                  isQueryTab && (mainTab as { kind: "query"; id: string }).id === qt.id
                    ? "border-primary text-foreground font-medium"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                )}
                onClick={() => { setMainTab({ kind: "query", id: qt.id }); setActiveQueryTabId(qt.id); }}
              >
                {qt.running && <RefreshCw className="h-2.5 w-2.5 animate-spin shrink-0" />}
                <span className="truncate max-w-[120px]">{qt.label}</span>
                {queryTabs.length > 1 && (
                  <button
                    className="ml-0.5 text-muted-foreground hover:text-foreground shrink-0"
                    onClick={(e) => { e.stopPropagation(); removeQueryTab(qt.id); }}
                  >
                    <X className="h-2.5 w-2.5" />
                  </button>
                )}
              </div>
            ))}

            <button
              className="px-1.5 py-1 text-xs text-muted-foreground hover:text-foreground shrink-0"
              title={t.database.newQuery}
              onClick={addQueryTab}
            >
              <Plus className="h-3 w-3" />
            </button>
          </div>

          {/* Content + optional console split */}
          <div className="flex-1 flex flex-col overflow-hidden">
            <div className="flex-1 overflow-hidden">
              {mainTab === "database" && selectedDatabase && (
                <DatabasePanel
                  dbName={selectedDatabase}
                  tables={tables[selectedDatabase] ?? []}
                  onSelectTable={(tbl) => void handleSelectTableFromDbPanel(tbl)}
                />
              )}

              {mainTab === "table" && selectedTableInfo && (
                <ColumnStructurePanel
                  tbl={selectedTableInfo}
                  def={tableDef}
                  sessionId={sessionIdRef.current}
                  database={selectedDatabase}
                  schema={isMysql ? (selectedDatabase ?? "public") : "public"}
                  isMysql={isMysql}
                  onRefresh={() => void handleSelectTable(selectedDatabase!, selectedTableInfo)}
                  onQueryError={setErrorModal}
                />
              )}

              {mainTab === "data" && selectedTableInfo && (
                <div className="flex flex-col h-full overflow-hidden">
                  <div className="flex items-center gap-2 px-2 py-0.5 border-b border-border bg-muted/20 shrink-0 text-[11px] text-muted-foreground">
                    {tableData && <span>{tableData.rows.length} {t.database.rowCount} · {tableData.duration_ms}{t.database.durationMs}</span>}
                    <span className="flex-1" />
                    <button className="hover:text-foreground" onClick={() => void loadTableData()} title={t.database.toolbarRefresh}>
                      <RefreshCw className="h-3 w-3" />
                    </button>
                  </div>
                  <div className="flex-1 overflow-hidden">
                    {tableDataLoading ? (
                      <div className="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground">
                        <RefreshCw className="h-4 w-4 animate-spin" />
                      </div>
                    ) : tableData ? (
                      <ResultsGrid
                        result={tableData}
                        tableDef={tableDef}
                        database={selectedDatabase ?? undefined}
                        schema={isMysql ? (selectedDatabase ?? undefined) : "public"}
                        table={selectedTableInfo.name}
                        sessionId={sessionIdRef.current}
                        isMysql={isMysql}
                        onContextMenu={setContextMenu}
                        onCellUpdated={() => void loadTableData()}
                      />
                    ) : null}
                  </div>
                </div>
              )}

              {isQueryTab && activeQueryTab && (
                <div className="flex flex-col h-full overflow-hidden">
                  <div className="flex-1 overflow-hidden" style={{ minHeight: "120px" }}>
                    <Editor
                      height="100%"
                      language="sql"
                      theme="vs-dark"
                      value={activeQueryTab.text}
                      onChange={(v) => updateQueryTab(activeQueryTabId, { text: v ?? "" })}
                      onMount={(editor) => {
                        queryEditorRef.current = editor;
                        editor.addCommand(2048 | 3, () => void runQuery());
                      }}
                      options={{
                        minimap: { enabled: false },
                        fontSize: 12,
                        lineNumbers: "on",
                        scrollbar: { vertical: "auto" },
                        wordWrap: "on",
                        padding: { top: 6, bottom: 6 },
                      }}
                    />
                  </div>
                  {activeQueryTab.result && (
                    <div className="flex flex-col border-t border-border" style={{ height: "40%" }}>
                      <div className="flex items-center gap-2 px-2 py-0.5 border-b border-border bg-muted/20 shrink-0 text-[11px] text-muted-foreground">
                        <span>{activeQueryTab.result.rows.length} {t.database.rowCount} · {activeQueryTab.result.duration_ms}{t.database.durationMs}</span>
                      </div>
                      <div className="flex-1 overflow-hidden">
                        <ResultsGrid result={activeQueryTab.result} onContextMenu={setContextMenu} />
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>

            {showConsole && (
              <div className="border-t border-border shrink-0" style={{ height: "160px" }}>
                <ConsolePanel lines={consoleLogs} />
              </div>
            )}
          </div>
        </div>
      </div>

      {contextMenu && (
        <TableContextMenu menu={contextMenu} onClose={() => setContextMenu(null)} />
      )}

      {errorModal && (
        <ErrorModal title="Erro na consulta" message={errorModal} onClose={() => setErrorModal(null)} />
      )}

      {showUserManager && sessionIdRef.current && (
        <UserManagerModal
          sessionId={sessionIdRef.current}
          databases={databases}
          onClose={() => setShowUserManager(false)}
        />
      )}
    </div>
  );
}
