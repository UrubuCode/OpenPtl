import { Eye, EyeOff, Folder } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { getError } from "@/functions/common";
import { useT } from "@/langs";
import { api } from "@/lib/tauri";
import { useAppStore } from "@/store/app-store";
import type { DatabaseDriver, DatabaseProfile } from "@/types/openptl";

const DEFAULT_PORTS: Record<DatabaseDriver, number> = {
  postgres: 5432,
  mysql: 3306,
  mariadb: 3306,
  sqlite: 0,
};

export function DatabaseFormDrawer() {
  const t = useT();
  const open = useAppStore((state) => state.databaseDrawerOpen);
  const draft = useAppStore((state) => state.databaseDraft);
  const closeDatabaseDrawer = useAppStore((state) => state.closeDatabaseDrawer);
  const saveDatabaseProfile = useAppStore((state) => state.saveDatabaseProfile);

  const [showPassword, setShowPassword] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<"success" | "error" | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const testGenRef = useRef(0);

  const { register, handleSubmit, watch, setValue, reset } = useForm<DatabaseProfile>({
    defaultValues: draft,
  });

  useEffect(() => {
    reset(draft);
    setTestResult(null);
    setTestError(null);
    // cancel any in-flight test when drawer resets
    testGenRef.current += 1;
    setTesting(false);
  }, [draft, reset]);

  const driver = watch("driver");
  const isSqlite = driver === "sqlite";

  // Cancel test when driver changes
  useEffect(() => {
    testGenRef.current += 1;
    setTesting(false);
    setTestResult(null);
    setTestError(null);
  }, [driver]);

  const onSubmit = handleSubmit(async (data) => {
    if (!data.name.trim()) {
      data.name = data.driver === "sqlite"
        ? (data.file_path ?? "sqlite")
        : `${data.host ?? ""}:${data.port ?? ""}`;
    }
    if (!data.port && !isSqlite) {
      data.port = DEFAULT_PORTS[data.driver];
    }
    await saveDatabaseProfile(data);
  });

  const testConnection = async () => {
    const gen = ++testGenRef.current;
    setTesting(true);
    setTestResult(null);
    setTestError(null);
    const data = watch();
    if (!data.name.trim()) data.name = "test";
    if (!data.port && !isSqlite) data.port = DEFAULT_PORTS[data.driver];
    const tmpProfile: DatabaseProfile = { ...data, id: "" };
    let savedId: string | null = null;
    try {
      const saved = await api.dbProfileSave(tmpProfile);
      savedId = saved.id;
      if (testGenRef.current !== gen) return;
      const sessionId = await api.dbConnect(saved.id);
      await api.dbDisconnect(sessionId);
      await api.dbProfileDelete(saved.id);
      savedId = null;
      if (testGenRef.current !== gen) return;
      setTestResult("success");
    } catch (err) {
      if (savedId) void api.dbProfileDelete(savedId).catch(() => undefined);
      if (testGenRef.current !== gen) return;
      setTestResult("error");
      setTestError(getError(err));
    } finally {
      if (testGenRef.current === gen) setTesting(false);
    }
  };

  const pickFile = async () => {
    const selected = await openDialog({ multiple: false, directory: false }).catch(() => null);
    if (selected && typeof selected === "string") {
      setValue("file_path", selected);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(v) => !v && closeDatabaseDrawer()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {draft.id ? t.database.drawerTitleEdit : t.database.drawerTitle}
          </DialogTitle>
        </DialogHeader>

        <form onSubmit={onSubmit} className="space-y-3">
          <input type="hidden" {...register("id")} />

          <div className="space-y-1">
            <Label className="text-xs">{t.database.fieldName}</Label>
            <Input {...register("name")} placeholder="My Database" className="h-8 text-sm" />
          </div>

          <div className="space-y-1">
            <Label className="text-xs">{t.database.fieldDriver}</Label>
            <Select
              value={driver}
              onValueChange={(v) => {
                setValue("driver", v as DatabaseDriver);
                setValue("port", DEFAULT_PORTS[v as DatabaseDriver] || null);
              }}
            >
              <SelectTrigger className="h-8 text-sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="postgres">PostgreSQL</SelectItem>
                <SelectItem value="mysql">MySQL</SelectItem>
                <SelectItem value="mariadb">MariaDB</SelectItem>
                <SelectItem value="sqlite">SQLite</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {isSqlite ? (
            <div className="space-y-1">
              <Label className="text-xs">{t.database.fieldFilePath}</Label>
              <div className="flex gap-2">
                <Input
                  {...register("file_path")}
                  placeholder="/path/to/database.db"
                  className="h-8 text-sm flex-1"
                />
                <Button type="button" size="sm" variant="outline" onClick={pickFile}>
                  <Folder className="h-4 w-4" />
                </Button>
              </div>
            </div>
          ) : (
            <>
              <div className="grid grid-cols-3 gap-2">
                <div className="col-span-2 space-y-1">
                  <Label className="text-xs">{t.database.fieldHost}</Label>
                  <Input {...register("host")} placeholder="localhost" className="h-8 text-sm" />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">{t.database.fieldPort}</Label>
                  <Input
                    {...register("port", { valueAsNumber: true })}
                    type="number"
                    placeholder={String(DEFAULT_PORTS[driver] || 5432)}
                    className="h-8 text-sm"
                  />
                </div>
              </div>

              <div className="space-y-1">
                <Label className="text-xs">{t.database.fieldUsername}</Label>
                <Input {...register("username")} placeholder="postgres" className="h-8 text-sm" autoComplete="off" />
              </div>

              <div className="space-y-1">
                <Label className="text-xs">{t.database.fieldPassword}</Label>
                <div className="relative">
                  <Input
                    {...register("password")}
                    type={showPassword ? "text" : "password"}
                    className="h-8 text-sm pr-8"
                    autoComplete="new-password"
                  />
                  <button
                    type="button"
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                    onClick={() => setShowPassword((v) => !v)}
                  >
                    {showPassword ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                  </button>
                </div>
              </div>

              <div className="space-y-1">
                <Label className="text-xs">{t.database.fieldDatabase}</Label>
                <Input {...register("database")} placeholder="mydb" className="h-8 text-sm" />
              </div>

              <div className="flex items-center gap-2">
                <Switch
                  checked={watch("ssl")}
                  onCheckedChange={(v) => setValue("ssl", v)}
                  id="db-ssl"
                />
                <Label htmlFor="db-ssl" className="text-xs cursor-pointer">{t.database.fieldSsl}</Label>
              </div>
            </>
          )}

          {testResult === "success" && (
            <p className="text-xs text-green-500">{t.database.testSuccess}</p>
          )}
          {testResult === "error" && (
            <p className="text-xs text-red-400">{t.database.testError}: {testError}</p>
          )}

          <DialogFooter className="flex gap-2 sm:gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={testConnection}
              disabled={testing}
            >
              {testing ? "..." : t.database.testConnection}
            </Button>
            <Button type="submit" size="sm">{t.common.save}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
