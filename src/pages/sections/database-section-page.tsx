import { Database, MoreHorizontal, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { useT } from "@/langs";
import { useAppStore } from "@/store/app-store";
import type { DatabaseProfile } from "@/types/openptl";

const DRIVER_LABELS: Record<DatabaseProfile["driver"], string> = {
  postgres: "PostgreSQL",
  mysql: "MySQL",
  mariadb: "MariaDB",
  sqlite: "SQLite",
};

const DRIVER_COLOR: Record<DatabaseProfile["driver"], string> = {
  postgres: "text-blue-400",
  mysql: "text-orange-400",
  mariadb: "text-orange-400",
  sqlite: "text-green-400",
};

export function DatabaseSectionPage() {
  const t = useT();
  const databaseProfiles = useAppStore((state) => state.databaseProfiles);
  const openDatabaseDrawer = useAppStore((state) => state.openDatabaseDrawer);
  const deleteDatabaseProfile = useAppStore((state) => state.deleteDatabaseProfile);
  const openDatabase = useAppStore((state) => state.openDatabase);
  const tabs = useAppStore((state) => state.tabs);

  const [search, setSearch] = useState("");

  const openProfileIds = useMemo(
    () => new Set(tabs.filter((t) => t.type === "database" && t.profileId).map((t) => t.profileId!)),
    [tabs],
  );

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return databaseProfiles;
    return databaseProfiles.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.driver.toLowerCase().includes(q) ||
        (p.host ?? "").toLowerCase().includes(q) ||
        (p.database ?? "").toLowerCase().includes(q),
    );
  }, [databaseProfiles, search]);

  const stats = useMemo(
    () => [
      { label: t.database.statsTotal, value: String(databaseProfiles.length), color: "text-foreground" },
      {
        label: t.database.statsMysql,
        value: String(databaseProfiles.filter((p) => p.driver === "mysql" || p.driver === "mariadb").length),
        color: "text-orange-400",
      },
      {
        label: t.database.statsPostgres,
        value: String(databaseProfiles.filter((p) => p.driver === "postgres").length),
        color: "text-blue-400",
      },
      {
        label: t.database.statsSqlite,
        value: String(databaseProfiles.filter((p) => p.driver === "sqlite").length),
        color: "text-green-400",
      },
    ],
    [databaseProfiles, t.database.statsMysql, t.database.statsPostgres, t.database.statsSqlite, t.database.statsTotal],
  );

  return (
    <div className="flex-1 p-6 space-y-6 h-full overflow-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-foreground">{t.database.sectionTitle}</h1>
          <p className="text-sm text-muted-foreground mt-0.5">{t.database.sectionSubtitle}</p>
        </div>
        <Button size="sm" className="gap-2" onClick={() => openDatabaseDrawer()}>
          <Plus className="h-4 w-4" />
          {t.database.newConnection}
        </Button>
      </div>

      {/* Search */}
      <div className="relative max-w-sm">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t.database.searchPlaceholder}
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

      {/* Cards */}
      {filtered.length > 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {filtered.map((profile) => {
            const isOpen = openProfileIds.has(profile.id);
            return (
              <DatabaseCard
                key={profile.id}
                profile={profile}
                isOpen={isOpen}
                onOpen={() => void openDatabase(profile)}
                onEdit={() => openDatabaseDrawer(profile)}
                onDelete={() => void deleteDatabaseProfile(profile.id)}
                statusLabel={isOpen ? t.database.statusOpen : t.database.statusIdle}
              />
            );
          })}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
          <Database className="h-12 w-12 text-muted-foreground/30" />
          <p className="text-sm text-muted-foreground">
            {databaseProfiles.length === 0 ? t.database.newConnection : "Nenhum resultado"}
          </p>
          {databaseProfiles.length === 0 && (
            <Button size="sm" variant="outline" onClick={() => openDatabaseDrawer()}>
              <Plus className="h-4 w-4 mr-1" />
              {t.database.newConnection}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

interface DatabaseCardProps {
  profile: DatabaseProfile;
  isOpen: boolean;
  statusLabel: string;
  onOpen: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

function DatabaseCard({ profile, isOpen, statusLabel, onOpen, onEdit, onDelete }: DatabaseCardProps) {
  const connectionString =
    profile.driver === "sqlite"
      ? profile.file_path ?? "—"
      : `${profile.username ?? ""}@${profile.host ?? ""}:${profile.port ?? ""}`;

  return (
    <div
      className="group relative rounded-xl border border-border/50 bg-card p-4 cursor-pointer transition-all hover:border-primary/40 hover:shadow-md hover:shadow-primary/5"
      onClick={onOpen}
    >
      {/* Header row */}
      <div className="flex items-start justify-between gap-2 mb-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <div className="h-8 w-8 rounded-lg bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Database className="h-4 w-4 text-yellow-500" />
          </div>
          <div className="min-w-0">
            <p className="text-sm font-medium text-foreground truncate">{profile.name}</p>
            <p className="text-xs text-muted-foreground truncate">{connectionString}</p>
          </div>
        </div>

        <details
          className="relative shrink-0"
          onClick={(e) => e.stopPropagation()}
        >
          <summary className="flex h-7 w-7 cursor-pointer list-none items-center justify-center rounded-md border border-border/50 text-muted-foreground hover:bg-accent hover:text-foreground transition-colors [&::-webkit-details-marker]:hidden">
            <MoreHorizontal className="h-3.5 w-3.5" />
          </summary>
          <div className="absolute right-0 top-8 z-20 min-w-[140px] rounded-lg border border-border bg-popover p-1 shadow-xl">
            <button
              type="button"
              className="w-full rounded-md px-2 py-1.5 text-left text-xs hover:bg-accent transition-colors"
              onClick={onEdit}
            >
              Editar
            </button>
            <button
              type="button"
              className="w-full rounded-md px-2 py-1.5 text-left text-xs text-destructive hover:bg-destructive/10 transition-colors"
              onClick={onDelete}
            >
              Remover
            </button>
          </div>
        </details>
      </div>

      {/* Footer row */}
      <div className="flex items-center justify-between">
        <span className={`text-xs font-medium ${DRIVER_COLOR[profile.driver]}`}>
          {DRIVER_LABELS[profile.driver]}
        </span>
        <div className="flex items-center gap-1.5">
          {profile.database && (
            <span className="text-[10px] text-muted-foreground font-mono bg-muted/50 px-1.5 py-0.5 rounded">
              {profile.database}
            </span>
          )}
          <span
            className={`text-[10px] font-medium px-1.5 py-0.5 rounded-full border ${
              isOpen
                ? "text-green-400 bg-green-500/10 border-green-500/20"
                : "text-muted-foreground bg-muted/30 border-border/30"
            }`}
          >
            {statusLabel}
          </span>
        </div>
      </div>
    </div>
  );
}
