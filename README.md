# OpenPtl

OpenPtl é um cliente desktop nativo para conexões SSH e operações SFTP, construído integralmente em **Rust** com **eframe/egui**. A aplicação não depende de runtime web: a interface, a orquestração de tarefas e os serviços de domínio são compilados no mesmo binário.

## Recursos principais

O cliente oferece um vault local criptografado para perfis e credenciais, proteção por senha mestre com Argon2id e XChaCha20-Poly1305, gerenciamento de conexões SSH/SFTP, shell local, terminal integrado, validação explícita de hosts desconhecidos e armazenamento seguro de `known_hosts`.

A interface principal é formada por telas nativas de visão geral, conexões, keychain, workspace, configurações e informações do produto. As operações de rede permanecem isoladas do código de apresentação e usam Tokio para executar tarefas assíncronas do protocolo SSH.

## Arquitetura

| Camada | Responsabilidade | Localização |
| --- | --- | --- |
| Entrada | Inicialização da janela nativa e ciclo egui | `src/main.rs` |
| Aplicação | Estado de navegação e casos de uso | `src/app.rs` |
| Backend | Coordenação de vault, SSH, SFTP e runtime assíncrono | `src/backend.rs` |
| Interface | Telas e componentes egui | `src/ui/` |
| Domínio | Modelos, vault criptografado e tarefas de transferência | `src/libs/` |
| Protocolos | Sessões SSH e adaptadores SFTP | `src/protocols/` |

Os arquivos Rust são mantidos abaixo de **500 linhas**, com módulos separados por responsabilidade. O formato binário do vault mantém os índices históricos dos enums para preservar a compatibilidade com dados existentes.

## Requisitos

É necessário ter o toolchain Rust estável, um compilador C compatível e as bibliotecas gráficas nativas da plataforma. Em distribuições Debian/Ubuntu, o ambiente básico pode ser preparado com `build-essential`, `pkg-config`, `libx11-dev`, `libxkbcommon-dev`, `libwayland-dev` e `libudev-dev`.

## Desenvolvimento

```bash
cargo run
```

Para uma compilação otimizada:

```bash
cargo build --release
```

Para executar os testes:

```bash
cargo test
```

Para validar estilo e problemas comuns:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Segurança operacional

A primeira execução solicita uma senha mestre com pelo menos seis caracteres. A senha é usada localmente para derivar a chave do vault e não é persistida em texto puro. Ao conectar a um servidor cuja chave ainda não é conhecida, a aplicação exibe a impressão digital e exige uma confirmação explícita antes de gravá-la.

As credenciais devem ser cadastradas no vault e nunca em arquivos de configuração do projeto. O arquivo `known_hosts` de trabalho é materializado a partir de uma cópia protegida dentro do vault e é recapturado após alterações da sessão.

## Licença

Este projeto é distribuído sob a licença MIT. Consulte `LICENSE` para o texto completo.
