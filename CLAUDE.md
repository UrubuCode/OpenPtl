# CLAUDE.md — OpenPtl

## Overview

Desktop application for managing remote connections (SSH, SFTP). Built with **Tauri 2 + React 19 + TypeScript** on the frontend and **Rust** on the backend.

## Project structure

```
src/                        # Frontend React/TypeScript
  App.tsx                   # Root: routing, global modals (sync auth, conflicts, etc.)
  store/
    app-store.ts            # Zustand store (global state)
    app-store.types.ts      # Store types
  functions/
    vault-actions.ts        # bootstrap, vaultInit/Unlock/Lock, loadWorkspace, runSync
    connection-actions.ts   # openSsh, openSftpWorkspace
    session-actions.ts      # ensureSessionListeners, disconnectSession, sshWrite
    sftp-editor-actions.ts  # openTab (editor), saveEditor
  pages/
    sections/               # Sidebar pages (home, keychain, known-hosts, notes, settings, etc.)
    tabs/
      workspace-tab-page.tsx  # Full workspace: SSH/SFTP/editor blocks, drag, transfers
      workspace/
        terminal.tsx          # TerminalBlockView (xterm.js)
        sftp.tsx              # SftpBlockView (file browser)
        editor.tsx            # EditorBlockView (Monaco)
        types.ts              # WorkspaceBlock, ConnectStage, etc.
  components/
    layout/                 # AppSidebar, AppHeader, WorkTabs
    workspace/              # WorkspaceBlockController (react-rnd)
    drawers/                # HostFormDrawer, KeychainFormDrawer
  langs/                    # i18n — see section below
  types/                    # openptl.ts (types shared between frontend and backend)
  lib/
    tauri.ts                # Wrapper for all Tauri commands (api.*)

src-tauri/src/              # Rust backend
  lib.rs                    # All registered Tauri commands (>2000 lines)
  libs/
    vault.rs                # Encrypted vault (Argon2 + AES-GCM), profiles, keychains
    sync.rs                 # Server sync (Google OAuth, push/pull)
    remote_fs.rs            # SFTP — remote file operations
    shared_fs.rs            # Unified local + remote file operations
    transfer.rs             # File transfers between endpoints
    key_actions.rs          # Global input capture (rdev) for SSH terminal
    task.rs                 # Internal async task runner
    models.rs               # Structs shared across libs

server/src/index.js         # Cloudflare Worker — Google OAuth broker for sync
```

## Essential commands

```bash
# Development
npm run tauri dev          # Start Tauri app with hot-reload

# Build
npm run build              # Build frontend (tsc + vite)
npm run tauri build        # Full build (frontend + Rust + installer)

# Type check
npx tsc --noEmit           # Check TypeScript types without compiling
```

## i18n — translations

All user-facing strings must be added to **both** translation files:

- `src/langs/en_US/` — English strings
- `src/langs/pt_BR/` — Portuguese (Brazil) strings

The shared type lives in `src/langs/types.ts` (`AppDictionary`). Adding a new string requires:
1. Add the key to the relevant interface in `types.ts`
2. Add the English value in the corresponding `en_US/*.ts` file
3. Add the Portuguese value in the corresponding `pt_BR/*.ts` file

**Never hardcode visible strings in Portuguese or English in component files.** Always use the `useT()` hook and reference a key from `AppDictionary`.

## Workspace architecture

The workspace uses floating blocks (react-rnd). Each block has:
- `connectStage: ConnectStage` — `"connecting" | "ready" | "error" | "verifying_fingerprint" | "awaiting_password"`
- `pendingProfileId` — profile pending connection
- Auto-retry with countdown via `connectRetryTimersRef`
- Cancel token via `connectCancelTokenRef` — incremented on cancel, checked after each `await`

Key functions in `workspace-tab-page.tsx`:
- `resolvePendingTerminalConnection` — connects SSH, manages stages, retry
- `resolvePendingSftpConnection` — connects SFTP over SSH

## Vault

All sensitive data (profiles, keychains, settings) is stored in an AES-GCM encrypted binary file. The key derives from the master password via Argon2. The vault can sync with a server via Google OAuth (`sync.rs`).

`bootstrap()` in `vault-actions.ts`:
1. Calls `api.syncCancel()` to cancel any in-progress sync (prevents softlock on F5)
2. Checks vault status
3. If unlocked, calls `loadWorkspace()` (may perform a sync pull on startup)

## Code conventions

- **No comments** unless the WHY is non-obvious
- Workspace callbacks passed as typed props (not context)
- Block state mutated only via `setBlocks((current) => current.map(...))`
- All backend calls via `api.*()` from `src/lib/tauri.ts`
- Workspace snapshots stored in `workspaceSnapshotsByTab` — blocks with `connectStage: "connecting"` are reset to `"error"` on restore

## Sync / Auth

- Login via server selection modal → `runSync("login", address)` → Google OAuth
- While logging in: `loginServerBusy = true`, but cancel is always allowed via `cancelLoginServer()`
- F5 during login: `syncCancel()` in bootstrap releases the backend lock

## Supported protocols

| Protocol | Backend | UI |
|----------|---------|-----|
| SSH | russh | TerminalBlock (xterm.js) |
| SFTP | russh-sftp | SftpBlock |

> RDP, VNC, FTP/FTPS, SMB, the SQL database feature, and the WebRTC streaming POC
> were removed from `main` to focus the base product on SSH/SFTP. The full
> multi-protocol + WebRTC work is preserved on the `features-plan` branch.

## Deep links

Format: `openptl://ssh/host:port` or direct `ssh://user@host:port`  
Processed in `App.tsx` via `parseConnectionDeepLink()` → queued into `pendingDeepLinks`.
