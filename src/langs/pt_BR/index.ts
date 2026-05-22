import type { AppDictionary } from "../types";
import { app } from "./app";
import { vault } from "./vault";
import { home } from "./home";
import { settings } from "./settings";
import { keychain } from "./keychain";
import { knownHosts } from "./known-hosts";
import { about } from "./about";
import { debugLogs } from "./debug-logs";
import { hostDrawer } from "./host-drawer";
import { workspace } from "./workspace";
import { editor } from "./editor";
import { conflicts } from "./conflicts";
import { toasts } from "./toasts";
import { common } from "./common";
import { backendMessages } from "./backend-messages";
import { notes } from "./notes";
import { database } from "./database";

export const ptBR: AppDictionary = {
  app,
  sidebar: {
    home: "Home",
    keychain: "Keychain",
    knownHosts: "Known Hosts",
    notes: "Notas",
    settings: "Configurações",
    debugLogs: "Logs",
    about: "Sobre",
    database: "Bancos de Dados",
    newConnection: "Nova conexão",
    groupMain: "Principal",
    groupSystem: "Sistema",
  },
  vault,
  home,
  settings,
  keychain,
  knownHosts,
  about,
  debugLogs,
  hostDrawer,
  workspace,
  editor,
  conflicts,
  toasts,
  backendMessages,
  common,
  notes,
  database,
};
