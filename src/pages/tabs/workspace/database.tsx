import {
  ChevronDown,
  ChevronRight,
  Database,
  Grid3X3,
  Layers,
  Play,
  RefreshCw,
  Table2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import Editor from "@monaco-editor/react";

import { useT } from "@/langs";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { DatabaseProfile, DbQueryResult, DbTableDefinition, DbTableInfo } from "@/types/openptl";
import type { WorkspaceBlockModule } from "./block-module";
import type { DatabaseBlock } from "./types";

export interface DatabaseBlockViewProps {
  block: DatabaseBlock;
  profile: DatabaseProfile | null;
  onFocus: () => void;
  onConnectStageChange: (
    stage: DatabaseBlock["connectStage"],
    message: string | null,
    error: string | null,
  ) => void;
  onBlockUpdate: (patch: Partial<DatabaseBlock>) => void;
  onViewModeChange: (mode: "tabular" | "object_explorer") => void;
}

function ValueCell({ value }: { value: unknown }) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic text-xs">NULL</span>;
  }
  if (typeof value === "boolean") {
    return <span className={cn("text-xs font-mono", value ? "text-green-500" : "text-red-500")}>{String(value)}</span>;
  }
  if (typeof value === "number") {
    return <span className="text-xs font-mono text-blue-400 text-right block">{String(value)}</span>;
  }
  const str = String(value);
  return <span className="text-xs font-mono truncate block max-w-[200px]" title={str}>{str}</span>;
}



function ResultsGrid({ result }: { result: DbQueryResult }) {
  const t = useT();

  if (result.rows.length === 0) {
    return (
      <div className="flex items-center justify-center h-20 text-sm text-muted-foreground">
        {t.database.noResults}
        {result.affected_rows > 0 && (
          <span className="ml-2">
            ({result.affected_rows} {t.database.affectedRows})
          </span>
        )}
      </div>
    );
  }

  return (
    <div className="overflow-auto h-full">
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
          {result.rows.map((row, ri) => (
            <tr key={ri} className="hover:bg-accent/30 border-b border-border/50">
              {row.map((cell, ci) => (
                <td key={ci} className="px-2 py-0.5 align-top">
                  <ValueCell value={cell} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TableDefinitionView({ def }: { def: DbTableDefinition }) {
  const t = useT();
  const [tab, setTab] = useState<"columns" | "indexes" | "fk" | "ddl">("columns");

  return (
    <div className="flex flex-col h-full">
      <div className="flex gap-1 border-b border-border px-2 pt-1">
        {(["columns", "indexes", "fk", "ddl"] as const).map((key) => (
          <button
            key={key}
            className={cn(
              "px-2 py-1 text-xs rounded-t",
              tab === key ? "bg-accent font-medium" : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() => setTab(key)}
          >
            {key === "columns"
              ? t.database.columns
              : key === "indexes"
                ? t.database.indexes
                : key === "fk"
                  ? t.database.foreignKeys
                  : t.database.ddl}
          </button>
        ))}
      </div>
      <div className="flex-1 overflow-auto p-2">
        {tab === "columns" && (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-1 px-2 text-muted-foreground">Name</th>
                <th className="text-left py-1 px-2 text-muted-foreground">Type</th>
                <th className="text-left py-1 px-2 text-muted-foreground">{t.database.nullable}</th>
                <th className="text-left py-1 px-2 text-muted-foreground">{t.database.defaultValue}</th>
                <th className="text-left py-1 px-2 text-muted-foreground">PK</th>
              </tr>
            </thead>
            <tbody>
              {def.columns.map((col, i) => (
                <tr key={i} className="border-b border-border/40 hover:bg-accent/20">
                  <td className="py-0.5 px-2 font-mono font-medium">{col.name}</td>
                  <td className="py-0.5 px-2 text-blue-400 font-mono">{col.data_type}</td>
                  <td className="py-0.5 px-2 text-center">{col.nullable ? "✓" : ""}</td>
                  <td className="py-0.5 px-2 text-muted-foreground italic">{col.default_value ?? ""}</td>
                  <td className="py-0.5 px-2 text-center">{col.is_primary_key ? "🔑" : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {tab === "indexes" && (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-1 px-2 text-muted-foreground">Name</th>
                <th className="text-left py-1 px-2 text-muted-foreground">Columns</th>
                <th className="text-left py-1 px-2 text-muted-foreground">Unique</th>
              </tr>
            </thead>
            <tbody>
              {def.indexes.map((idx, i) => (
                <tr key={i} className="border-b border-border/40 hover:bg-accent/20">
                  <td className="py-0.5 px-2 font-mono">{idx.name}</td>
                  <td className="py-0.5 px-2">{idx.columns.join(", ")}</td>
                  <td className="py-0.5 px-2 text-center">{idx.unique ? "✓" : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {tab === "fk" && (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left py-1 px-2 text-muted-foreground">Name</th>
                <th className="text-left py-1 px-2 text-muted-foreground">Columns</th>
                <th className="text-left py-1 px-2 text-muted-foreground">References</th>
              </tr>
            </thead>
            <tbody>
              {def.foreign_keys.map((fk, i) => (
                <tr key={i} className="border-b border-border/40 hover:bg-accent/20">
                  <td className="py-0.5 px-2 font-mono text-xs">{fk.name}</td>
                  <td className="py-0.5 px-2">{fk.columns.join(", ")}</td>
                  <td className="py-0.5 px-2">
                    {fk.ref_table}({fk.ref_columns.join(", ")})
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {tab === "ddl" && (
          <pre className="text-xs font-mono whitespace-pre-wrap text-muted-foreground">
            {def.ddl ?? "DDL not available"}
          </pre>
        )}
      </div>
    </div>
  );
}

export function DatabaseBlockView({
  block,
  profile,
  onFocus,
  onConnectStageChange,
  onBlockUpdate,
  onViewModeChange,
}: DatabaseBlockViewProps) {
  const t = useT();
  const queryEditorRef = useRef<unknown>(null);

  const [databases, setDatabases] = useState<string[]>([]);
  const [schemas, setSchemas] = useState<Record<string, string[]>>({});
  const [tables, setTables] = useState<Record<string, DbTableInfo[]>>({});
  const [loadingNode, setLoadingNode] = useState<string | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(new Set());
  const [tableDef, setTableDef] = useState<DbTableDefinition | null>(null);
  const [tableDefKey, setTableDefKey] = useState<string | null>(null);
  const [showDef, setShowDef] = useState(false);

  const sessionId = block.sessionId;

  const loadDatabases = useCallback(async () => {
    if (!sessionId) return;
    setLoadingNode("root");
    try {
      const dbs = await api.dbListDatabases(sessionId);
      setDatabases(dbs);
    } finally {
      setLoadingNode(null);
    }
  }, [sessionId]);

  useEffect(() => {
    if (sessionId && databases.length === 0) {
      void loadDatabases();
    }
  }, [sessionId, databases.length, loadDatabases]);

  const handleToggleNode = useCallback(
    async (key: string) => {
      setExpandedNodes((prev) => {
        const next = new Set(prev);
        if (next.has(key)) {
          next.delete(key);
        } else {
          next.add(key);
        }
        return next;
      });

      if (!sessionId) return;
      const parts = key.split("::");
      if (parts[0] === "db" && !schemas[parts[1]]) {
        setLoadingNode(key);
        try {
          const dbName = parts[1];
          const s = await api.dbListSchemas(sessionId, dbName);
          setSchemas((prev) => ({ ...prev, [dbName]: s }));
        } finally {
          setLoadingNode(null);
        }
      } else if (parts[0] === "schema" && !tables[key]) {
        setLoadingNode(key);
        try {
          const [dbName, schemaName] = [parts[1], parts[2]];
          const tbl = await api.dbListTables(sessionId, dbName, schemaName);
          setTables((prev) => ({ ...prev, [key]: tbl }));
        } finally {
          setLoadingNode(null);
        }
      }
    },
    [sessionId, schemas, tables],
  );

  const handleSelectTable = useCallback(
    async (db: string, schema: string, table: string) => {
      if (!sessionId) return;
      onBlockUpdate({
        selectedDatabase: db,
        selectedSchema: schema,
        selectedTable: table,
      });
      const key = `${db}::${schema}::${table}`;
      if (tableDefKey !== key) {
        setTableDefKey(key);
        setTableDef(null);
        setShowDef(false);
        const def = await api.dbDescribeTable(sessionId, db, schema, table).catch(() => null);
        setTableDef(def);
      }
    },
    [sessionId, onBlockUpdate, tableDefKey],
  );

  const runQuery = useCallback(async () => {
    if (!sessionId || block.queryRunning) return;
    onBlockUpdate({ queryRunning: true, queryError: null, queryResult: null });
    try {
      const result = await api.dbQuery(sessionId, block.queryText);
      onBlockUpdate({ queryResult: result, queryRunning: false });
    } catch (err) {
      onBlockUpdate({
        queryError: err instanceof Error ? err.message : String(err),
        queryRunning: false,
      });
    }
  }, [sessionId, block.queryRunning, block.queryText, onBlockUpdate]);

  const browseTable = useCallback(
    async (db: string, schema: string, table: string) => {
      if (!sessionId) return;
      onBlockUpdate({ queryRunning: true, queryError: null, queryResult: null });
      try {
        const result = await api.dbGetTableData(sessionId, db, schema, table, 200, 0);
        onBlockUpdate({ queryResult: result, queryRunning: false });
      } catch (err) {
        onBlockUpdate({
          queryError: err instanceof Error ? err.message : String(err),
          queryRunning: false,
        });
      }
    },
    [sessionId, onBlockUpdate],
  );

  if (block.connectStage === "connecting") {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground gap-2">
        <RefreshCw className="h-4 w-4 animate-spin" />
        {t.database.connecting}
      </div>
    );
  }

  if (block.connectStage === "error") {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3">
        <Database className="h-8 w-8 text-destructive" />
        <p className="text-sm text-destructive">{block.connectError ?? t.database.errorConnecting}</p>
        <button
          className="px-3 py-1 text-sm border border-border rounded hover:bg-accent"
          onClick={() => onConnectStageChange("connecting", t.database.connecting, null)}
        >
          <RefreshCw className="h-3 w-3 inline mr-1" />
          Retry
        </button>
      </div>
    );
  }

  const isTabular = block.viewMode === "tabular";

  return (
    <div className="flex flex-col h-full overflow-hidden" onMouseDown={onFocus}>
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-border px-2 py-1 shrink-0">
        <Database className="h-4 w-4 text-muted-foreground" />
        <span className="text-xs font-medium truncate flex-1">
          {profile?.name ?? "Database"}
        </span>
        <div className="flex gap-1 border border-border rounded overflow-hidden">
          <button
            className={cn(
              "px-2 py-0.5 text-xs flex items-center gap-1",
              isTabular ? "bg-accent" : "hover:bg-accent/50",
            )}
            onClick={() => onViewModeChange("tabular")}
            title={t.database.tabularMode}
          >
            <Grid3X3 className="h-3 w-3" />
          </button>
          <button
            className={cn(
              "px-2 py-0.5 text-xs flex items-center gap-1",
              !isTabular ? "bg-accent" : "hover:bg-accent/50",
            )}
            onClick={() => onViewModeChange("object_explorer")}
            title={t.database.explorerMode}
          >
            <Layers className="h-3 w-3" />
          </button>
        </div>
      </div>

      {isTabular ? (
        <TabularView
          block={block}
          databases={databases}
          schemas={schemas}
          tables={tables}
          expandedNodes={expandedNodes}
          loadingNode={loadingNode}
          tableDef={tableDef}
          showDef={showDef}
          onToggleNode={handleToggleNode}
          onSelectTable={handleSelectTable}
          onBrowseTable={browseTable}
          onShowDef={setShowDef}
          onQueryChange={(q) => onBlockUpdate({ queryText: q })}
          onRunQuery={runQuery}
          onBlockUpdate={onBlockUpdate}
          queryEditorRef={queryEditorRef}
        />
      ) : (
        <ExplorerView
          block={block}
          databases={databases}
          schemas={schemas}
          tables={tables}
          onSelectTable={handleSelectTable}
          onBrowseTable={browseTable}
          onLoadSchemas={async (db) => {
            if (!sessionId || schemas[db]) return;
            const s = await api.dbListSchemas(sessionId, db);
            setSchemas((prev) => ({ ...prev, [db]: s }));
          }}
          onLoadTables={async (db, schema) => {
            const key = `schema::${db}::${schema}`;
            if (!sessionId || tables[key]) return;
            const tbl = await api.dbListTables(sessionId, db, schema);
            setTables((prev) => ({ ...prev, [key]: tbl }));
          }}
          onBlockUpdate={onBlockUpdate}
          onRunQuery={runQuery}
        />
      )}

      {/* Results panel (shared by both modes) */}
      {(block.queryResult || block.queryRunning || block.queryError) && (
        <div className="flex flex-col border-t border-border" style={{ height: "40%" }}>
          <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-muted/30 shrink-0">
            <span className="text-xs text-muted-foreground flex-1">
              {block.queryResult &&
                `${block.queryResult.rows.length} ${t.database.rowCount} · ${block.queryResult.duration_ms}${t.database.durationMs}`}
            </span>
            {block.queryRunning && <RefreshCw className="h-3 w-3 animate-spin text-muted-foreground" />}
          </div>
          <div className="flex-1 overflow-hidden">
            {block.queryError && (
              <div className="p-2 text-xs text-destructive font-mono">{block.queryError}</div>
            )}
            {block.queryResult && !block.queryError && (
              <ResultsGrid result={block.queryResult} />
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function TabularView({
  block,
  databases,
  schemas,
  tables,
  expandedNodes,
  loadingNode,
  tableDef,
  showDef,
  onToggleNode,
  onSelectTable,
  onBrowseTable,
  onShowDef,
  onQueryChange,
  onRunQuery,
  onBlockUpdate,
  queryEditorRef,
}: {
  block: DatabaseBlock;
  databases: string[];
  schemas: Record<string, string[]>;
  tables: Record<string, DbTableInfo[]>;
  expandedNodes: Set<string>;
  loadingNode: string | null;
  tableDef: DbTableDefinition | null;
  showDef: boolean;
  onToggleNode: (key: string) => void;
  onSelectTable: (db: string, schema: string, table: string) => void;
  onBrowseTable: (db: string, schema: string, table: string) => void;
  onShowDef: (v: boolean) => void;
  onQueryChange: (q: string) => void;
  onRunQuery: () => void;
  onBlockUpdate: (patch: Partial<DatabaseBlock>) => void;
  queryEditorRef: React.MutableRefObject<unknown>;
}) {
  const t = useT();

  return (
    <div className="flex flex-1 overflow-hidden">
      {/* Sidebar tree */}
      <div className="w-56 shrink-0 border-r border-border overflow-auto bg-sidebar text-sidebar-foreground">
        <div className="p-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide">
          {t.database.databases}
        </div>
        {loadingNode === "root" && (
          <div className="px-3 py-1 text-xs text-muted-foreground">{t.database.loadingTree}</div>
        )}
        {databases.map((db) => {
          const dbKey = `db::${db}`;
          const dbExpanded = expandedNodes.has(dbKey);
          return (
            <div key={db}>
              <div
                className={cn(
                  "flex items-center gap-1 px-2 py-0.5 cursor-pointer hover:bg-accent/50 rounded text-sm",
                  block.selectedDatabase === db && !block.selectedTable && "bg-accent",
                )}
                onClick={() => onBlockUpdate({ selectedDatabase: db, selectedTable: null, selectedSchema: null })}
              >
                <button
                  className="shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleNode(dbKey);
                  }}
                >
                  {dbExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                </button>
                <Database className="h-3.5 w-3.5 shrink-0 text-yellow-500" />
                <span className="truncate text-xs">{db}</span>
              </div>
              {dbExpanded &&
                (schemas[db] ?? []).map((schema) => {
                  const schemaKey = `schema::${db}::${schema}`;
                  const schemaExpanded = expandedNodes.has(schemaKey);
                  return (
                    <div key={schema}>
                      <div
                        className="flex items-center gap-1 pl-6 pr-2 py-0.5 cursor-pointer hover:bg-accent/50 rounded text-xs"
                        onClick={() => onBlockUpdate({ selectedDatabase: db, selectedSchema: schema, selectedTable: null })}
                      >
                        <button
                          className="shrink-0"
                          onClick={(e) => {
                            e.stopPropagation();
                            onToggleNode(schemaKey);
                          }}
                        >
                          {schemaExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                        </button>
                        <Layers className="h-3.5 w-3.5 shrink-0 text-blue-400" />
                        <span className="truncate">{schema}</span>
                      </div>
                      {schemaExpanded &&
                        (tables[schemaKey] ?? []).map((tbl) => (
                          <div
                            key={tbl.name}
                            className={cn(
                              "flex items-center gap-1 pl-10 pr-2 py-0.5 cursor-pointer hover:bg-accent/50 rounded text-xs",
                              block.selectedTable === tbl.name &&
                                block.selectedDatabase === db &&
                                block.selectedSchema === schema &&
                                "bg-accent",
                            )}
                            onDoubleClick={() => onBrowseTable(db, schema, tbl.name)}
                            onClick={() => onSelectTable(db, schema, tbl.name)}
                          >
                            <Table2 className="h-3.5 w-3.5 shrink-0 text-green-400" />
                            <span className="truncate">{tbl.name}</span>
                            {tbl.kind === "view" && (
                              <span className="ml-auto text-[9px] text-muted-foreground">view</span>
                            )}
                          </div>
                        ))}
                      {schemaExpanded && loadingNode === schemaKey && (
                        <div className="pl-10 text-xs text-muted-foreground py-0.5">{t.database.loadingTree}</div>
                      )}
                    </div>
                  );
                })}
              {dbExpanded && loadingNode === dbKey && (
                <div className="pl-6 text-xs text-muted-foreground py-0.5">{t.database.loadingTree}</div>
              )}
            </div>
          );
        })}
      </div>

      {/* Main panel */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* SQL editor + run button */}
        <div className="flex flex-col border-b border-border" style={{ height: "35%" }}>
          <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-muted/20 shrink-0">
            <span className="text-xs text-muted-foreground flex-1">SQL</span>
            {block.selectedTable && (
              <>
                <button
                  className="text-xs text-muted-foreground hover:text-foreground px-1"
                  onClick={() =>
                    block.selectedTable &&
                    block.selectedDatabase &&
                    block.selectedSchema &&
                    onBrowseTable(block.selectedDatabase, block.selectedSchema, block.selectedTable)
                  }
                >
                  {t.database.browseData}
                </button>
                <button
                  className={cn("text-xs px-1", showDef ? "text-foreground" : "text-muted-foreground hover:text-foreground")}
                  onClick={() => onShowDef(!showDef)}
                >
                  {t.database.describeTable}
                </button>
              </>
            )}
            <button
              className="flex items-center gap-1 px-2 py-0.5 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90 disabled:opacity-50"
              onClick={onRunQuery}
              disabled={block.queryRunning || !block.queryText.trim()}
            >
              <Play className="h-3 w-3" />
              {t.database.runQuery}
            </button>
          </div>
          <div className="flex-1 overflow-hidden">
            {showDef && tableDef ? (
              <TableDefinitionView def={tableDef} />
            ) : (
              <Editor
                height="100%"
                language="sql"
                theme="vs-dark"
                value={block.queryText}
                onChange={(v) => onQueryChange(v ?? "")}
                onMount={(editor) => {
                  queryEditorRef.current = editor;
                  editor.addCommand(2048 | 3 /* Ctrl+Enter */, onRunQuery);
                }}
                options={{
                  minimap: { enabled: false },
                  fontSize: 12,
                  lineNumbers: "off",
                  scrollbar: { vertical: "hidden" },
                  overviewRulerLanes: 0,
                  wordWrap: "on",
                  padding: { top: 4, bottom: 4 },
                }}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function ExplorerView({
  block,
  databases,
  schemas,
  tables,
  onSelectTable,
  onBrowseTable,
  onLoadSchemas,
  onLoadTables,
  onBlockUpdate,
  onRunQuery,
}: {
  block: DatabaseBlock;
  databases: string[];
  schemas: Record<string, string[]>;
  tables: Record<string, DbTableInfo[]>;
  onSelectTable: (db: string, schema: string, table: string) => void;
  onBrowseTable: (db: string, schema: string, table: string) => void;
  onLoadSchemas: (db: string) => Promise<void>;
  onLoadTables: (db: string, schema: string) => Promise<void>;
  onBlockUpdate: (patch: Partial<DatabaseBlock>) => void;
  onRunQuery: () => void;
}) {
  const t = useT();
  const { selectedDatabase, selectedSchema, selectedTable } = block;

  useEffect(() => {
    if (selectedDatabase && !schemas[selectedDatabase]) {
      void onLoadSchemas(selectedDatabase);
    }
  }, [selectedDatabase, schemas, onLoadSchemas]);

  useEffect(() => {
    if (selectedDatabase && selectedSchema) {
      const key = `schema::${selectedDatabase}::${selectedSchema}`;
      if (!tables[key]) {
        void onLoadTables(selectedDatabase, selectedSchema);
      }
    }
  }, [selectedDatabase, selectedSchema, tables, onLoadTables]);

  const breadcrumbs = [
    selectedDatabase && { label: selectedDatabase, onClick: () => onBlockUpdate({ selectedSchema: null, selectedTable: null }) },
    selectedSchema && { label: selectedSchema, onClick: () => onBlockUpdate({ selectedTable: null }) },
    selectedTable && { label: selectedTable, onClick: () => {} },
  ].filter(Boolean) as { label: string; onClick: () => void }[];

  return (
    <div className="flex-1 overflow-auto p-3">
      {/* Breadcrumb */}
      {breadcrumbs.length > 0 && (
        <div className="flex items-center gap-1 text-xs mb-3 text-muted-foreground">
          <button className="hover:text-foreground" onClick={() => onBlockUpdate({ selectedDatabase: null, selectedSchema: null, selectedTable: null })}>
            {t.database.databases}
          </button>
          {breadcrumbs.map((crumb, i) => (
            <span key={i} className="flex items-center gap-1">
              <ChevronRight className="h-3 w-3" />
              <button className={i === breadcrumbs.length - 1 ? "text-foreground font-medium" : "hover:text-foreground"} onClick={crumb.onClick}>
                {crumb.label}
              </button>
            </span>
          ))}
        </div>
      )}

      {/* Database list */}
      {!selectedDatabase && (
        <div className="grid grid-cols-2 gap-2">
          {databases.map((db) => (
            <button
              key={db}
              className="flex items-center gap-2 p-3 border border-border rounded-lg hover:bg-accent text-left"
              onClick={() => onBlockUpdate({ selectedDatabase: db, selectedSchema: null, selectedTable: null })}
            >
              <Database className="h-5 w-5 text-yellow-500 shrink-0" />
              <span className="text-sm font-medium truncate">{db}</span>
            </button>
          ))}
        </div>
      )}

      {/* Schema list */}
      {selectedDatabase && !selectedSchema && (
        <div className="grid grid-cols-2 gap-2">
          {(schemas[selectedDatabase] ?? []).map((schema) => (
            <button
              key={schema}
              className="flex items-center gap-2 p-3 border border-border rounded-lg hover:bg-accent text-left"
              onClick={() => onBlockUpdate({ selectedSchema: schema, selectedTable: null })}
            >
              <Layers className="h-5 w-5 text-blue-400 shrink-0" />
              <div>
                <div className="text-sm font-medium">{schema}</div>
                <div className="text-xs text-muted-foreground">
                  {tables[`schema::${selectedDatabase}::${schema}`]?.length ?? "?"} tables
                </div>
              </div>
            </button>
          ))}
        </div>
      )}

      {/* Table list */}
      {selectedDatabase && selectedSchema && !selectedTable && (
        <div className="grid grid-cols-2 gap-2">
          {(tables[`schema::${selectedDatabase}::${selectedSchema}`] ?? []).map((tbl) => (
            <button
              key={tbl.name}
              className="flex items-center gap-2 p-3 border border-border rounded-lg hover:bg-accent text-left"
              onDoubleClick={() => onBrowseTable(selectedDatabase, selectedSchema, tbl.name)}
              onClick={() => onSelectTable(selectedDatabase, selectedSchema, tbl.name)}
            >
              <Table2 className="h-5 w-5 text-green-400 shrink-0" />
              <div>
                <div className="text-sm font-medium">{tbl.name}</div>
                <div className="text-xs text-muted-foreground capitalize">{tbl.kind}</div>
              </div>
            </button>
          ))}
          {(tables[`schema::${selectedDatabase}::${selectedSchema}`] ?? []).length === 0 && (
            <p className="text-sm text-muted-foreground col-span-2">{t.database.emptyDatabase}</p>
          )}
        </div>
      )}

      {/* Table detail */}
      {selectedDatabase && selectedSchema && selectedTable && (
        <div className="flex flex-col gap-3">
          <div className="flex gap-2">
            <button
              className="px-3 py-1.5 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90"
              onClick={() => onBrowseTable(selectedDatabase, selectedSchema, selectedTable)}
            >
              {t.database.browseData}
            </button>
            <button
              className="px-3 py-1.5 text-xs border border-border rounded hover:bg-accent"
              onClick={() => {
                onBlockUpdate({
                  queryText: `SELECT * FROM "${selectedTable}" LIMIT 100;`,
                });
                onRunQuery();
              }}
            >
              SELECT *
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

const databaseWorkspaceModule: WorkspaceBlockModule<DatabaseBlockViewProps> = {
  name: "database",
  description: "Database client block (SQL query + object explorer).",
  render: (props) => <DatabaseBlockView {...props} />,
  onNotFound: ({ blockId, action }) =>
    `Database block not found (${action}) [${blockId}]`,
  onFailureLoad: ({ blockId, action }) =>
    `Database block load failed (${action}) [${blockId}]`,
};

export default databaseWorkspaceModule;
