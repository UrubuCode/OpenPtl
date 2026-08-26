## Overview

OpenPtl é uma aplicação desktop nativa para gerenciamento de conexões SSH e SFTP. A interface e o backend são implementados em **Rust estável**, usando **eframe/egui** para a janela e Tokio para as operações assíncronas.

## Project structure

```text
src/main.rs                 # Entrada do binário e configuração da janela nativa
src/app.rs                  # Estado de aplicação, navegação e casos de uso
src/backend.rs              # Fachada para vault, sessões SSH, SFTP e known_hosts
src/constants.rs            # Limites, nomes de arquivos e URLs do domínio
src/ui/                     # Telas egui separadas por responsabilidade
  layout.rs                 # Shell visual, navegação e barra de status
  vault_gate.rs             # Inicialização e desbloqueio do vault
  home.rs                   # Visão geral e ações rápidas
  connections.rs            # Listagem e edição de perfis
  connection_form.rs        # Formulário reutilizável de conexão
  keychain.rs               # Credenciais protegidas
  workspace.rs              # Sessões e terminal
  settings.rs               # Preferências persistidas
  challenges.rs             # Confirmação de impressão digital SSH
src/libs/models/             # Modelos serializados divididos por domínio
src/libs/vault/              # Persistência criptografada e ciclo de vida do vault
src/libs/task.rs             # Executor assíncrono de tarefas
src/libs/transfer.rs         # Métricas e adaptação de transferências
src/libs/remote_fs.rs        # Staging de arquivos remotos
src/protocols/ssh/           # Sessões, autenticação, terminal e known_hosts
src/protocols/sftp.rs        # Adaptador de operações SFTP
```

## Essential commands

```bash
cargo run
cargo build --release
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Code conventions

Cada arquivo Rust deve permanecer abaixo de **500 linhas**. Separe tipos, persistência, transporte, aplicação e apresentação em módulos diferentes. Prefira funções pequenas, nomes explícitos, tratamento de erros com contexto e interfaces orientadas ao domínio. Não acople a UI diretamente aos detalhes de armazenamento ou do protocolo.

Toda alteração que tocar o formato binário do vault precisa preservar a ordem histórica dos enums e incluir um teste de compatibilidade. Segredos não podem aparecer em logs, mensagens de erro, arquivos de configuração ou snapshots de interface.

A camada `backend` é a única fachada usada pela UI para acessar o vault e as sessões. A UI deve apenas transformar ações do usuário em chamadas dessa fachada e manter estado de apresentação; regras de negócio pertencem aos módulos de domínio.

## Vault

O vault armazena perfis, credenciais e configurações em arquivos binários criptografados. A senha mestre deriva a chave com Argon2id. O conteúdo é protegido com XChaCha20-Poly1305. O arquivo de trabalho `known_hosts` é materializado localmente e recapturado para o armazenamento protegido após as alterações de sessão.

O fluxo de inicialização exige confirmação da senha. O fluxo de conexão SSH bloqueia hosts desconhecidos até que o usuário confirme explicitamente a impressão digital apresentada pelo servidor.

## Supported protocols

| Protocol | Backend | UI |
| --- | --- | --- |
| SSH | `russh` | `src/ui/workspace.rs` |
| SFTP | `russh-sftp` | Fachada em `src/backend.rs` e adaptador em `src/protocols/sftp.rs` |

## Production checklist

Antes de uma entrega, execute formatação, testes, Clippy com warnings como erro e uma compilação release. Revise as mudanças com `git diff`, confirme que não há referências a stacks removidos e valide manualmente inicialização, desbloqueio, criação de conexão, confirmação de host, shell local, bloqueio e reabertura do vault.
