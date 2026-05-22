import { toast } from "sonner";

import { getError } from "@/functions/common";
import type { StoreGet, StoreSet } from "@/functions/store-types";
import { api } from "@/lib/tauri";
import type { AppActions } from "@/store/app-store.types";
import type { DatabaseProfile } from "@/types/openptl";

const BLANK_DATABASE_PROFILE: DatabaseProfile = {
  id: "",
  name: "",
  driver: "postgres",
  host: null,
  port: null,
  username: null,
  password: null,
  database: null,
  file_path: null,
  ssl: false,
  view_mode: "tabular",
};

export { BLANK_DATABASE_PROFILE };

export function createDatabaseActions(
  set: StoreSet,
  get: StoreGet,
): Pick<
  AppActions,
  | "openDatabaseDrawer"
  | "closeDatabaseDrawer"
  | "saveDatabaseProfile"
  | "deleteDatabaseProfile"
  | "openDatabase"
> {
  return {
    openDatabaseDrawer: (profile?: DatabaseProfile) => {
      set({
        databaseDrawerOpen: true,
        databaseDraft: profile ?? { ...BLANK_DATABASE_PROFILE },
      });
    },

    closeDatabaseDrawer: () => {
      set({ databaseDrawerOpen: false });
    },

    saveDatabaseProfile: async (profile: DatabaseProfile) => {
      try {
        const saved = await api.dbProfileSave(profile);
        set((state) => {
          const next = state.databaseProfiles.filter((p) => p.id !== saved.id);
          next.push(saved);
          next.sort((a, b) => a.name.localeCompare(b.name));
          return { databaseProfiles: next, databaseDrawerOpen: false };
        });
      } catch (error) {
        toast.error(getError(error));
      }
    },

    deleteDatabaseProfile: async (id: string) => {
      try {
        await api.dbProfileDelete(id);
        set((state) => ({
          databaseProfiles: state.databaseProfiles.filter((p) => p.id !== id),
        }));
      } catch (error) {
        toast.error(getError(error));
      }
    },

    openDatabase: async (profile: DatabaseProfile) => {
      get().openTab({
        id: `database:${Date.now()}:${Math.random().toString(16).slice(2, 7)}`,
        type: "database",
        title: `DB - ${profile.name}`,
        closable: true,
        profileId: profile.id,
      });
    },
  };
}
