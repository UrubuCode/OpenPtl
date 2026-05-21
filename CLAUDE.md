# CLAUDE.md — OpenPtl

## Visão geral

Aplicação desktop de gerenciamento de conexões remotas (SSH, SFTP, FTP/FTPS, SMB, RDP). Construída com **Tauri 2 + React 19 + TypeScript** no frontend e **Rust** no backend.

## Estrutura do projeto

```
src/                        # Frontend React/TypeScript
  App.tsx                   # Root: roteamento, modais globais (sync auth, conflitos, etc.)
  store/
    app-store.ts            # Zustand store (estado global)
    app-store.types.ts      # Tipos do store
  functions/
    vault-actions.ts        # bootstrap, vaultInit/Unlock/Lock, loadWorkspace, runSync
    connection-actions.ts   # openSsh, openSftpWorkspace, openRdp
    session-actions.ts      # ensureSessionListeners, disconnectSession, sshWrite
    sftp-editor-actions.ts  # openTab (editor), saveEditor
  pages/
    sections/               # Páginas da sidebar (home, keychain, known-hosts, notes, settings, etc.)
    tabs/
      workspace-tab-page.tsx  # Workspace completo: blocos SSH/SFTP/RDP/editor, drag, transfers
      workspace/
        terminal.tsx          # TerminalBlockView (xterm.js)
        sftp.tsx              # SftpBlockView (file browser)
        rdp.tsx               # RdpBlockView (pixi.js + IronRDP stream)
        editor.tsx            # EditorBlockView (Monaco)
        types.ts              # WorkspaceBlock, ConnectStage, etc.
  components/
    layout/                 # AppSidebar, AppHeader, WorkTabs
    workspace/              # WorkspaceBlockController (react-rnd)
    drawers/                # HostFormDrawer, KeychainFormDrawer
  langs/                    # i18n (en_US, pt_BR)
  types/                    # openptl.ts (tipos compartilhados frontend/backend)
  lib/
    tauri.ts                # Wrapper de todos os comandos Tauri (api.*)

src-tauri/src/              # Backend Rust
  lib.rs                    # Todos os comandos Tauri registrados (>2000 linhas)
  libs/
    vault.rs                # Cofre criptografado (Argon2 + AES-GCM), perfis, keychains
    sync.rs                 # Sincronização com servidor (Google OAuth, push/pull)
    remote_fs.rs            # SFTP/FTP/FTPS/SMB — operações de arquivo remoto
    shared_fs.rs            # Operações de arquivo locais + remoto unificadas
    transfer.rs             # Transferências de arquivo entre endpoints
    key_actions.rs          # Captura de input global (rdev) para RDP/SSH
    task.rs                 # Task runner assíncrono interno
    models.rs               # Structs compartilhadas entre libs

server/src/index.js         # Cloudflare Worker — broker OAuth Google para sync
```

## Comandos essenciais

```bash
# Desenvolvimento
npm run tauri dev          # Inicia app Tauri com hot-reload

# Build
npm run build              # Build frontend (tsc + vite)
npm run tauri build        # Build completo (frontend + Rust + instalador)

# Type check
npx tsc --noEmit           # Verifica tipos TypeScript sem compilar
```

## Arquitetura de workspace

O workspace usa blocos flutuantes (react-rnd). Cada bloco tem:
- `connectStage: ConnectStage` — `"connecting" | "ready" | "error" | "verifying_fingerprint" | "awaiting_password"`
- `pendingProfileId` — perfil aguardando conexão
- Retry automático com countdown via `connectRetryTimersRef`
- Cancel token via `connectCancelTokenRef` — incrementado no cancel, verificado após cada `await`

Funções chave em `workspace-tab-page.tsx`:
- `resolvePendingTerminalConnection` — conecta SSH, gerencia stages, retry
- `resolvePendingSftpConnection` — conecta SFTP via SSH
- `resolvePendingRdpConnection` — conecta RDP via IronRDP

## Vault (cofre)

Todo dado sensível (perfis, keychains, settings) fica em arquivo binário cifrado com AES-GCM. A chave deriva da master password via Argon2. O vault pode sincronizar com servidor via Google OAuth (`sync.rs`).

`bootstrap()` em `vault-actions.ts`:
1. Chama `api.syncCancel()` para cancelar qualquer sync em andamento (previne softlock no F5)
2. Chega status do vault
3. Se desbloqueado, chama `loadWorkspace()` (pode fazer sync pull na startup)

## Padrões de código

- **Sem comentários** salvo WHY não-óbvio
- Callbacks de workspace passados via props tipadas (não context)
- Estado de bloco mutado somente via `setBlocks((current) => current.map(...))`
- Chamadas ao backend: sempre `api.*()` de `src/lib/tauri.ts`
- i18n: `useT()` hook — nunca strings hardcoded visíveis ao usuário (exceto logs internos)
- Snapshots de workspace salvos em `workspaceSnapshotsByTab` — blocos com `connectStage: "connecting"` são resetados para `"error"` no restore

## Sincronização / Auth

- Login via modal de seleção de servidor → `runSync("login", address)` → OAuth Google
- Durante login: `loginServerBusy = true`, mas cancelar é sempre permitido via `cancelLoginServer()`
- F5 durante login: `syncCancel()` no bootstrap libera o lock do backend

## Protocolos suportados

| Protocolo | Backend | UI |
|-----------|---------|-----|
| SSH | russh | TerminalBlock (xterm.js) |
| SFTP | russh-sftp | SftpBlock |
| FTP/FTPS | suppaftp | SftpBlock |
| SMB | pavao | SftpBlock |
| RDP | IronRDP | RdpBlock (pixi.js stream) |

## Deep links

Formato: `openptl://ssh/host:port` ou direto `ssh://user@host:port`  
Processados em `App.tsx` via `parseConnectionDeepLink()` → enfileirados em `pendingDeepLinks`.
