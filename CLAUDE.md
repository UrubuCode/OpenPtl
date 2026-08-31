# CLAUDE.md — OpenPtl

## Visão geral

OpenPtl é uma aplicação nativa multiplataforma para gerenciamento de conexões SSH e SFTP. O backend é **Rust estável**, a interface é declarativa em **Slint** e as operações assíncronas usam Tokio. Não há WebView, Node, npm nem bundler: o binário desenha a própria interface.

## Estrutura do projeto

```text
build.rs                      # Compila ui/main.slint para Rust
ui/
  main.slint                  # AppWindow: só propriedades, callbacks e composição
  theme/{tokens,typography,palette}.slint
  models/                     # Structs compartilhadas com o Rust
  components/                 # Primitivos: UiButton, UiField, UiCard, UiBadge, UiToggle
  patterns/                   # Modal, ListPanel, PageHeader, TransferPanel
  layout/{sidebar,app-shell}.slint
  pages/                      # Uma tela por arquivo
src/
  main.rs                     # Entrada do binário
  constants.rs                # Limites, nomes de arquivo e URLs do domínio
  backend/                    # Fachada única da UI para o domínio
    mod.rs                    # Vault, conexões, SSH, SFTP, transferências
    sync.rs                   # Sincronização com o Drive
    update.rs                 # Consulta e download de atualizações
  libs/
    models/                   # Modelos serializados por domínio
    mutations/                # Log de mutações: HLC, operações, estado CRDT
    vault/                    # Persistência criptografada e ciclo de vida
    sync/                     # OAuth, Drive, layout remoto, servidores oficiais
    terminal.rs               # Adaptador do alacritty_terminal
    editor.rs                 # Adaptador do cosmic-text
    transfer.rs               # Fila e métricas de transferência
    deeplink.rs               # Endereços que abrem o aplicativo
    updater.rs                # Manifesto e verificação minisign
    secret_store.rs           # Keychain do sistema
  protocols/
    ssh/                      # Sessões, autenticação, terminal, known_hosts
    sftp.rs                   # Adaptador SFTP
  ui/
    bridge.rs                 # Registro dos callbacks Slint
    mappers.rs                # Tradução domínio <-> Slint
    *_flow.rs                 # Um fluxo por área da interface
server/                       # Worker Cloudflare: broker de OAuth do Google
```

## Comandos essenciais

```bash
cargo run
cargo build --release
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Convenções de código

Cada arquivo Rust deve ficar abaixo de **500 linhas** e cada `.slint` abaixo de **250**. Separe tipos, persistência, transporte, aplicação e apresentação em módulos diferentes. Prefira funções pequenas, nomes explícitos, erros com contexto e interfaces orientadas ao domínio.

A camada `backend` é a única fachada usada pela UI. A interface transforma ações do usuário em chamadas dessa fachada e mantém apenas estado de apresentação; regra de negócio pertence aos módulos de domínio. A UI nunca conhece tokio, russh, cosmic-text nem alacritty_terminal.

Nenhum trabalho bloqueante roda num callback do Slint. Operações longas vão para o runtime e voltam pelo event loop via `upgrade_in_event_loop`. Toda closure de callback usa `as_weak()`; capturar o handle forte cria ciclo e vaza a janela.

Para organização da camada Slint, veja a skill `slint-frontend`.

## Vault

O vault guarda perfis, credenciais, notas e configurações em arquivos binários criptografados. A senha mestre deriva a chave com Argon2id e o conteúdo é protegido com XChaCha20-Poly1305.

**O formato local é posicional (bincode).** Acrescentar um campo a uma struct já persistida invalida todos os vaults existentes. Dado novo vai para um arquivo `.bin` próprio, como `known_hosts.bin` e `notes.bin` fazem. Alteração que toque o formato precisa preservar a ordem histórica dos enums e incluir teste de compatibilidade. A exceção é `mutations.bin`, que usa JSON cifrado: o mapa CRDT guarda valores de campo como `serde_json::Value`, que um formato posicional não sabe reler.

O nonce do XChaCha20-Poly1305 é sempre aleatório. Derivá-lo do conteúdo revelava quais arquivos guardavam a mesma coisa e não sobreviveria ao log, onde lotes distintos podem ter conteúdo igual.

## Sincronização

O Drive não oferece compare-and-swap, então **nada compartilhado é reescrito**. Cada alteração local vira um lote imutável de mutações num arquivo `<uuidv7>.bin`; a convergência sai do relógio lógico dentro do payload, nunca da ordem de upload ou do `createdTime`, que marcam quando o arquivo subiu e não quando a mudança aconteceu.

A pasta remota tem três coisas: `header.bin` com salt e verificador da chave mestre — sem segredo, mas é o que permite a um aparelho novo derivar a mesma chave; `snapshot-<uuidv7>.bin` com o estado completo, publicado na compactação; e os lotes. O nome dos lotes é opaco de propósito: prefixá-los com o dispositivo entregaria ao Google quantos aparelhos existem e qual muda mais.

O fluxo é **local-first**: a alteração vale neste aparelho antes de qualquer rede e fica na fila de envio. Enviar primeiro deixaria o aplicativo inutilizável offline sem ganhar segurança nenhuma. Mutações são granulares por campo e a remoção é lápide; remover de verdade faria uma mutação atrasada ressuscitar o registro.

A lista de servidores de autenticação é buscada em `auth-servers.json` no repositório a cada login e mesclada com a que o usuário cadastrou. A oficial acrescenta e atualiza; nunca apaga o que é local, e servidores marcados `from_remote` não entram no log — eles têm origem própria.

Segredos não podem aparecer em logs, mensagens de erro, arquivos de configuração ou snapshots de interface. Listas na interface carregam resumo; o conteúdo completo só sai do cofre para preencher um formulário de edição.

O fluxo de inicialização exige confirmação da senha. A conexão SSH bloqueia hosts desconhecidos até o usuário confirmar a impressão digital apresentada pelo servidor, e o `known_hosts` só é recapturado para o cofre depois que a sessão abre.

## Protocolos e bibliotecas

| Área | Biblioteca | Motivo |
| --- | --- | --- |
| SSH | `russh` | |
| SFTP | `russh-sftp` | |
| Terminal | `alacritty_terminal` | grade, histórico e escapes prontos e testados |
| Editor | `cosmic-text` + `syntect` | `TextEdit` do Slint não aplica estilo por trecho |
| Atualização | `minisign-verify` | mesma chave que assinava as releases do Tauri |

O editor rasteriza num `SharedPixelBuffer` exibido como `Image`. Slint também expõe `set_rendering_notifier` com `GraphicsAPI::NativeOpenGL` para desenho em GL cru, caso alguma superfície precise.

## Segurança

Um deep link **preenche o formulário e para**: não grava no cofre e não conecta. Um endereço vindo de fora não deve apontar o aplicativo para um servidor arbitrário sem o usuário ver e confirmar.

A atualização só chega ao disco depois de a assinatura minisign conferir, e a instalação é sempre um clique explícito. Baixar do Drive só substitui o cofre local quando o remoto traz conteúdo.

## Antes de entregar

Execute formatação, testes, Clippy com warnings como erro e uma compilação release. Revise com `git diff`, confirme que não há referências a Tauri, WebView, npm ou React, e valide manualmente inicialização, desbloqueio, criação de conexão, confirmação de host, terminal, navegação SFTP, bloqueio e reabertura do vault.
