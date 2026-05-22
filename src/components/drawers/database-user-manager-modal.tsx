import {
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  User,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";

// ─── Privilege lists ──────────────────────────────────────────────────────────

const GLOBAL_PRIVS = [
  "SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP",
  "PROCESS", "FILE", "REFERENCES", "INDEX", "ALTER",
  "SHOW DATABASES", "CREATE TEMPORARY TABLES", "LOCK TABLES",
  "EXECUTE", "CREATE VIEW", "SHOW VIEW", "CREATE ROUTINE", "ALTER ROUTINE",
  "CREATE USER", "EVENT", "TRIGGER", "SUPER", "RELOAD",
  "REPLICATION SLAVE", "REPLICATION CLIENT", "GRANT OPTION",
];

const DB_PRIVS = [
  "SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP",
  "REFERENCES", "INDEX", "ALTER", "CREATE TEMPORARY TABLES", "LOCK TABLES",
  "EXECUTE", "CREATE VIEW", "SHOW VIEW", "CREATE ROUTINE", "ALTER ROUTINE",
  "EVENT", "TRIGGER", "GRANT OPTION",
];

// ─── Types ────────────────────────────────────────────────────────────────────

interface MysqlUser {
  user: string;
  host: string;
  maxQuestions: number;
  maxUpdates: number;
  maxConnections: number;
  maxUserConnections: number;
}

export interface UserManagerModalProps {
  sessionId: string;
  databases: string[];
  onClose: () => void;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async function runSql(sessionId: string, sql: string): Promise<unknown[][]> {
  try {
    const result = await api.dbQuery(sessionId, sql);
    return result.rows;
  } catch {
    return [];
  }
}

// Escape for standard SQL string literals (doubles single quotes)
function esc(v: string) {
  return v.replace(/'/g, "''");
}

// Build a SQL string literal that matches a GRANTEE column value like 'user'@'host'
// The stored value has literal single quotes, so in a SQL literal they become ''
function granteeExpr(user: string, host: string) {
  return `'''${esc(user)}''@''${esc(host)}'''`;
}

// ─── Main modal ───────────────────────────────────────────────────────────────

export function UserManagerModal({ sessionId, databases, onClose }: UserManagerModalProps) {
  const [users, setUsers] = useState<MysqlUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<MysqlUser | null>(null);
  const [tab, setTab] = useState<"credentials" | "limits">("credentials");

  // Credential form
  const [formUser, setFormUser] = useState("");
  const [formHost, setFormHost] = useState("%");
  const [formPassword, setFormPassword] = useState("");
  const [formConfirm, setFormConfirm] = useState("");

  // Limits form
  const [formMaxQueries, setFormMaxQueries] = useState(0);
  const [formMaxUpdates, setFormMaxUpdates] = useState(0);
  const [formMaxConnections, setFormMaxConnections] = useState(0);
  const [formMaxUserConnections, setFormMaxUserConnections] = useState(0);

  // Privileges
  const [globalPrivs, setGlobalPrivs] = useState<Set<string>>(new Set());
  const [dbPrivsMap, setDbPrivsMap] = useState<Record<string, Set<string>>>({});
  const [selectedPrivDb, setSelectedPrivDb] = useState("");
  const [privLoading, setPrivLoading] = useState(false);

  // UI
  const [saving, setSaving] = useState(false);
  const [statusMsg, setStatusMsg] = useState<{ text: string; isError: boolean } | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newUser, setNewUser] = useState({ user: "", host: "%", password: "" });
  const [confirmRemove, setConfirmRemove] = useState(false);

  const showStatus = (text: string, isError = false) => {
    setStatusMsg({ text, isError });
    setTimeout(() => setStatusMsg(null), 4000);
  };

  // CONVERT(...USING utf8mb4) forces binary MySQL columns to come back as strings
  const loadUsers = useCallback(async () => {
    setLoading(true);
    const rows = await runSql(
      sessionId,
      "SELECT CONVERT(User USING utf8mb4), CONVERT(Host USING utf8mb4), " +
        "max_questions, max_updates, max_connections, max_user_connections " +
        "FROM mysql.user ORDER BY User, Host",
    );
    setUsers(
      rows.map((r) => ({
        user: String(r[0] ?? ""),
        host: String(r[1] ?? ""),
        maxQuestions: Number(r[2] ?? 0),
        maxUpdates: Number(r[3] ?? 0),
        maxConnections: Number(r[4] ?? 0),
        maxUserConnections: Number(r[5] ?? 0),
      })),
    );
    setLoading(false);
  }, [sessionId]);

  useEffect(() => {
    void loadUsers();
  }, [loadUsers]);

  const loadGrants = useCallback(
    async (user: string, host: string) => {
      setPrivLoading(true);
      const ge = granteeExpr(user, host);

      const globalRows = await runSql(
        sessionId,
        `SELECT PRIVILEGE_TYPE FROM information_schema.USER_PRIVILEGES WHERE GRANTEE = ${ge}`,
      );
      setGlobalPrivs(new Set(globalRows.map((r) => String(r[0]))));

      const dbRows = await runSql(
        sessionId,
        `SELECT TABLE_SCHEMA, PRIVILEGE_TYPE FROM information_schema.SCHEMA_PRIVILEGES WHERE GRANTEE = ${ge}`,
      );
      const dbMap: Record<string, Set<string>> = {};
      for (const r of dbRows) {
        const db = String(r[0]);
        if (!dbMap[db]) dbMap[db] = new Set();
        dbMap[db].add(String(r[1]));
      }
      setDbPrivsMap(dbMap);
      setPrivLoading(false);
    },
    [sessionId],
  );

  const selectUser = useCallback(
    (u: MysqlUser) => {
      setSelected(u);
      setFormUser(u.user);
      setFormHost(u.host);
      setFormPassword("");
      setFormConfirm("");
      setFormMaxQueries(u.maxQuestions);
      setFormMaxUpdates(u.maxUpdates);
      setFormMaxConnections(u.maxConnections);
      setFormMaxUserConnections(u.maxUserConnections);
      setConfirmRemove(false);
      setStatusMsg(null);
      void loadGrants(u.user, u.host);
    },
    [loadGrants],
  );

  const saveCredentials = async () => {
    if (!selected) return;
    if (formPassword && formPassword !== formConfirm) {
      showStatus("Senhas não coincidem", true);
      return;
    }
    setSaving(true);
    try {
      if (formUser !== selected.user || formHost !== selected.host) {
        await api.dbQuery(
          sessionId,
          `RENAME USER '${esc(selected.user)}'@'${esc(selected.host)}' TO '${esc(formUser)}'@'${esc(formHost)}'`,
        );
      }
      if (formPassword) {
        await api.dbQuery(
          sessionId,
          `ALTER USER '${esc(formUser)}'@'${esc(formHost)}' IDENTIFIED BY '${esc(formPassword)}'`,
        );
        setFormPassword("");
        setFormConfirm("");
      }
      showStatus("Credenciais salvas");
      setSelected((prev) => (prev ? { ...prev, user: formUser, host: formHost } : null));
      await loadUsers();
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    } finally {
      setSaving(false);
    }
  };

  const saveLimits = async () => {
    if (!selected) return;
    setSaving(true);
    try {
      await api.dbQuery(
        sessionId,
        `ALTER USER '${esc(formUser)}'@'${esc(formHost)}' WITH ` +
          `MAX_QUERIES_PER_HOUR ${formMaxQueries} ` +
          `MAX_UPDATES_PER_HOUR ${formMaxUpdates} ` +
          `MAX_CONNECTIONS_PER_HOUR ${formMaxConnections} ` +
          `MAX_USER_CONNECTIONS ${formMaxUserConnections}`,
      );
      showStatus("Limitações salvas");
      await loadUsers();
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    } finally {
      setSaving(false);
    }
  };

  const toggleGlobalPriv = async (priv: string) => {
    if (!selected) return;
    const q = `'${esc(selected.user)}'@'${esc(selected.host)}'`;
    const hasAll = globalPrivs.has("ALL PRIVILEGES");
    const has = priv === "ALL PRIVILEGES" ? hasAll : (hasAll || globalPrivs.has(priv));
    try {
      if (has) {
        await api.dbQuery(sessionId, `REVOKE ${priv} ON *.* FROM ${q}`);
        if (priv === "ALL PRIVILEGES") {
          setGlobalPrivs(new Set());
        } else {
          setGlobalPrivs((prev) => { const n = new Set(prev); n.delete(priv); return n; });
        }
      } else {
        await api.dbQuery(sessionId, `GRANT ${priv} ON *.* TO ${q}`);
        if (priv === "ALL PRIVILEGES") {
          // reload to reflect all individual privs now granted
          await loadGrants(selected.user, selected.host);
        } else {
          setGlobalPrivs((prev) => new Set([...prev, priv]));
        }
      }
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    }
  };

  const toggleDbPriv = async (priv: string) => {
    if (!selected || !selectedPrivDb) return;
    const q = `'${esc(selected.user)}'@'${esc(selected.host)}'`;
    const currentPrivs = dbPrivsMap[selectedPrivDb] ?? new Set<string>();
    const hasAll = currentPrivs.has("ALL PRIVILEGES");
    const has = priv === "ALL PRIVILEGES" ? hasAll : (hasAll || currentPrivs.has(priv));
    try {
      if (has) {
        await api.dbQuery(sessionId, `REVOKE ${priv} ON \`${selectedPrivDb}\`.* FROM ${q}`);
        setDbPrivsMap((prev) => {
          const n = { ...prev, [selectedPrivDb]: new Set(prev[selectedPrivDb]) };
          if (priv === "ALL PRIVILEGES") n[selectedPrivDb] = new Set();
          else n[selectedPrivDb].delete(priv);
          return n;
        });
      } else {
        await api.dbQuery(sessionId, `GRANT ${priv} ON \`${selectedPrivDb}\`.* TO ${q}`);
        if (priv === "ALL PRIVILEGES") {
          await loadGrants(selected.user, selected.host);
        } else {
          setDbPrivsMap((prev) => ({
            ...prev,
            [selectedPrivDb]: new Set([...(prev[selectedPrivDb] ?? []), priv]),
          }));
        }
      }
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    }
  };

  const addUser = async () => {
    if (!newUser.user.trim()) return;
    try {
      const sql = newUser.password
        ? `CREATE USER '${esc(newUser.user)}'@'${esc(newUser.host)}' IDENTIFIED BY '${esc(newUser.password)}'`
        : `CREATE USER '${esc(newUser.user)}'@'${esc(newUser.host)}'`;
      await api.dbQuery(sessionId, sql);
      setShowAddForm(false);
      setNewUser({ user: "", host: "%", password: "" });
      await loadUsers();
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    }
  };

  const removeUser = async () => {
    if (!selected) return;
    try {
      await api.dbQuery(sessionId, `DROP USER '${esc(selected.user)}'@'${esc(selected.host)}'`);
      setSelected(null);
      setConfirmRemove(false);
      await loadUsers();
    } catch (e) {
      showStatus(e instanceof Error ? e.message : String(e), true);
    }
  };

  // ── Priv state helpers ──
  const isGlobal = selectedPrivDb === "";
  const activePrivList = isGlobal ? GLOBAL_PRIVS : DB_PRIVS;
  const activePrivSet = isGlobal ? globalPrivs : (dbPrivsMap[selectedPrivDb] ?? new Set<string>());
  const hasAllPriv = activePrivSet.has("ALL PRIVILEGES");
  const isPrivChecked = (priv: string) => hasAllPriv || activePrivSet.has(priv);
  const togglePriv = isGlobal ? toggleGlobalPriv : toggleDbPriv;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className="bg-background border border-border rounded-xl shadow-2xl flex flex-col overflow-hidden"
        style={{ width: 940, height: 660 }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-border bg-muted/20 shrink-0">
          <ShieldCheck className="h-4 w-4 text-primary shrink-0" />
          <span className="font-semibold text-sm">Gerenciar Usuários</span>
          <span className="flex-1" />
          <button onClick={onClose}>
            <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
          </button>
        </div>

        {/* Body */}
        <div className="flex flex-1 overflow-hidden">
          {/* ── User list ── */}
          <div className="w-52 shrink-0 border-r border-border flex flex-col bg-sidebar text-sidebar-foreground">
            <div className="flex items-center gap-1 px-2 py-1.5 border-b border-border shrink-0">
              <button
                className="flex items-center gap-1 px-2 py-0.5 text-xs border border-border rounded hover:bg-accent"
                onClick={() => setShowAddForm((v) => !v)}
              >
                <Plus className="h-3 w-3" /> Adicionar
              </button>
              {!confirmRemove ? (
                <button
                  disabled={!selected}
                  className="flex items-center gap-1 px-2 py-0.5 text-xs border border-border rounded hover:bg-destructive/10 hover:text-destructive disabled:opacity-40"
                  onClick={() => setConfirmRemove(true)}
                >
                  <Trash2 className="h-3 w-3" /> Remover
                </button>
              ) : (
                <div className="flex gap-1">
                  <button
                    className="px-1.5 py-0.5 text-xs bg-destructive text-destructive-foreground rounded hover:bg-destructive/90"
                    onClick={() => void removeUser()}
                  >
                    Confirmar
                  </button>
                  <button
                    className="px-1.5 py-0.5 text-xs border border-border rounded hover:bg-accent"
                    onClick={() => setConfirmRemove(false)}
                  >
                    Cancelar
                  </button>
                </div>
              )}
            </div>

            {showAddForm && (
              <div className="px-2 py-2 border-b border-border bg-muted/10 space-y-1 shrink-0">
                <input
                  className="w-full bg-background border border-border rounded px-1.5 py-0.5 outline-none text-xs"
                  placeholder="Usuário"
                  value={newUser.user}
                  onChange={(e) => setNewUser((p) => ({ ...p, user: e.target.value }))}
                  onKeyDown={(e) => { if (e.key === "Enter") void addUser(); }}
                />
                <input
                  className="w-full bg-background border border-border rounded px-1.5 py-0.5 outline-none text-xs"
                  placeholder="Host (%)"
                  value={newUser.host}
                  onChange={(e) => setNewUser((p) => ({ ...p, host: e.target.value }))}
                />
                <input
                  type="password"
                  className="w-full bg-background border border-border rounded px-1.5 py-0.5 outline-none text-xs"
                  placeholder="Senha (opcional)"
                  value={newUser.password}
                  onChange={(e) => setNewUser((p) => ({ ...p, password: e.target.value }))}
                  onKeyDown={(e) => { if (e.key === "Enter") void addUser(); }}
                />
                <div className="flex gap-1">
                  <button
                    className="flex-1 py-0.5 bg-primary text-primary-foreground rounded text-xs hover:bg-primary/90"
                    onClick={() => void addUser()}
                  >
                    Criar
                  </button>
                  <button
                    className="flex-1 py-0.5 border border-border rounded text-xs hover:bg-accent"
                    onClick={() => { setShowAddForm(false); setNewUser({ user: "", host: "%", password: "" }); }}
                  >
                    Cancelar
                  </button>
                </div>
              </div>
            )}

            <div className="flex-1 overflow-auto text-xs">
              {loading ? (
                <div className="flex items-center justify-center h-16 text-muted-foreground">
                  <RefreshCw className="h-3 w-3 animate-spin" />
                </div>
              ) : (
                users.map((u, i) => (
                  <div
                    key={i}
                    className={cn(
                      "flex items-center gap-2 px-2 py-1.5 cursor-pointer hover:bg-accent/50 border-b border-border/30",
                      selected?.user === u.user && selected?.host === u.host && "bg-accent",
                    )}
                    onClick={() => selectUser(u)}
                  >
                    <User className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                    <div className="min-w-0">
                      <div className="truncate font-medium">{u.user}</div>
                      <div className="truncate text-[10px] text-muted-foreground">{u.host}</div>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>

          {/* ── Right panel ── */}
          {selected ? (
            <div className="flex-1 flex flex-col overflow-hidden">
              {/* Tab strip */}
              <div className="flex items-center border-b border-border px-3 pt-1.5 shrink-0 bg-muted/5">
                {(["credentials", "limits"] as const).map((t) => (
                  <button
                    key={t}
                    className={cn(
                      "px-3 py-1 text-xs border-b-2 -mb-px transition-colors whitespace-nowrap",
                      tab === t
                        ? "border-primary text-foreground font-medium"
                        : "border-transparent text-muted-foreground hover:text-foreground",
                    )}
                    onClick={() => setTab(t)}
                  >
                    {t === "credentials" ? "Credenciais" : "Limitações"}
                  </button>
                ))}
              </div>

              {/* Tab content — grid layouts */}
              <div className="p-4 shrink-0 border-b border-border">
                {tab === "credentials" && (
                  <div className="grid grid-cols-2 gap-3 max-w-xl">
                    <div>
                      <label className="text-xs text-muted-foreground block mb-0.5">Nome de usuário</label>
                      <input
                        className="w-full bg-background border border-border rounded px-2 py-1 text-sm outline-none focus:border-primary"
                        value={formUser}
                        onChange={(e) => setFormUser(e.target.value)}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground block mb-0.5">Servidor (%)</label>
                      <input
                        className="w-full bg-background border border-border rounded px-2 py-1 text-sm outline-none focus:border-primary"
                        value={formHost}
                        onChange={(e) => setFormHost(e.target.value)}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground block mb-0.5">Nova senha</label>
                      <input
                        type="password"
                        className="w-full bg-background border border-border rounded px-2 py-1 text-sm outline-none focus:border-primary"
                        placeholder="Deixe em branco para não alterar"
                        value={formPassword}
                        onChange={(e) => setFormPassword(e.target.value)}
                      />
                    </div>
                    <div>
                      <label className="text-xs text-muted-foreground block mb-0.5">Confirmar senha</label>
                      <input
                        type="password"
                        className="w-full bg-background border border-border rounded px-2 py-1 text-sm outline-none focus:border-primary"
                        value={formConfirm}
                        onChange={(e) => setFormConfirm(e.target.value)}
                      />
                    </div>
                    <div className="col-span-2">
                      <button
                        disabled={saving}
                        className="flex items-center gap-1.5 px-4 py-1.5 bg-primary text-primary-foreground rounded text-xs hover:bg-primary/90 disabled:opacity-50"
                        onClick={() => void saveCredentials()}
                      >
                        {saving && <RefreshCw className="h-3 w-3 animate-spin" />}
                        Salvar
                      </button>
                    </div>
                  </div>
                )}

                {tab === "limits" && (
                  <div className="grid grid-cols-2 gap-3 max-w-xl">
                    {([
                      ["Consultas por hora", formMaxQueries, setFormMaxQueries],
                      ["Atualizações por hora", formMaxUpdates, setFormMaxUpdates],
                      ["Conexões por hora", formMaxConnections, setFormMaxConnections],
                      ["Conexões simultâneas", formMaxUserConnections, setFormMaxUserConnections],
                    ] as [string, number, (v: number) => void][]).map(([label, val, setter]) => (
                      <div key={label}>
                        <label className="text-xs text-muted-foreground block mb-0.5">{label}</label>
                        <div className="flex items-center gap-2">
                          <input
                            type="number"
                            min={0}
                            className="w-28 bg-background border border-border rounded px-2 py-1 text-sm outline-none focus:border-primary"
                            value={val}
                            onChange={(e) => setter(Number(e.target.value))}
                          />
                          <span className="text-[11px] text-muted-foreground">0 = ilimitado</span>
                        </div>
                      </div>
                    ))}
                    <div className="col-span-2">
                      <button
                        disabled={saving}
                        className="flex items-center gap-1.5 px-4 py-1.5 bg-primary text-primary-foreground rounded text-xs hover:bg-primary/90 disabled:opacity-50"
                        onClick={() => void saveLimits()}
                      >
                        {saving && <RefreshCw className="h-3 w-3 animate-spin" />}
                        Salvar
                      </button>
                    </div>
                  </div>
                )}
              </div>

              {/* ── Permissions panel ── */}
              <div className="flex-1 flex flex-col overflow-hidden min-h-0">
                <div className="flex items-center gap-3 px-3 py-1.5 border-b border-border bg-muted/10 shrink-0">
                  <span className="text-xs font-semibold">Permissões</span>
                  <div className="flex items-center gap-1.5 ml-1">
                    <span className="text-xs text-muted-foreground">Banco:</span>
                    <select
                      className="bg-background border border-border rounded px-1.5 py-0.5 text-xs outline-none"
                      value={selectedPrivDb}
                      onChange={(e) => setSelectedPrivDb(e.target.value)}
                    >
                      <option value="">Global (*.*)</option>
                      {databases.map((db) => (
                        <option key={db} value={db}>{db}</option>
                      ))}
                    </select>
                  </div>
                  {privLoading && <RefreshCw className="h-3 w-3 animate-spin text-muted-foreground" />}
                  <button
                    className="ml-auto text-muted-foreground hover:text-foreground"
                    title="Recarregar permissões"
                    onClick={() => void loadGrants(selected.user, selected.host)}
                  >
                    <RefreshCw className="h-3.5 w-3.5" />
                  </button>
                </div>

                <div className="flex-1 overflow-auto px-3 py-2">
                  {/* ALL PRIVILEGES row at top */}
                  <label className="flex items-center gap-1.5 text-xs cursor-pointer hover:bg-accent/40 rounded px-1 py-0.5 mb-1 border-b border-border/50 pb-1.5 select-none font-medium">
                    <input
                      type="checkbox"
                      checked={hasAllPriv}
                      onChange={() => void togglePriv("ALL PRIVILEGES")}
                      className="accent-primary shrink-0"
                    />
                    ALL PRIVILEGES
                  </label>

                  <div className="grid grid-cols-3 gap-x-3 gap-y-0.5 sm:grid-cols-4">
                    {activePrivList.map((priv) => (
                      <label
                        key={priv}
                        className="flex items-center gap-1.5 text-xs cursor-pointer hover:bg-accent/40 rounded px-1 py-0.5 select-none"
                      >
                        <input
                          type="checkbox"
                          checked={isPrivChecked(priv)}
                          onChange={() => void togglePriv(priv)}
                          className="accent-primary shrink-0"
                        />
                        <span className="truncate">{priv}</span>
                      </label>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
              Selecione um usuário na lista
            </div>
          )}
        </div>

        {/* Status bar */}
        {statusMsg && (
          <div
            className={cn(
              "px-4 py-1.5 text-xs border-t border-border shrink-0",
              statusMsg.isError
                ? "text-destructive bg-destructive/10"
                : "text-green-500 bg-green-500/10",
            )}
          >
            {statusMsg.text}
          </div>
        )}
      </div>
    </div>
  );
}
