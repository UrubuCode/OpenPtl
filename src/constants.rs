//! Constantes de domínio compartilhadas entre vault, protocolos e apresentação.

/// Serviço usado como namespace no keychain do sistema operacional.
pub const APP_KEYRING_SERVICE: &str = "com.urubucode.openptl";
/// Chave que guarda o material criptográfico do vault no keychain.
pub const KEYRING_VAULT_KEY: &str = "vault-key";

/// Pasta raiz do armazenamento criptografado local.
pub const STORAGE_DIR_NAME: &str = "OpenPtl";
/// Pasta que agrupa os cofres. Cada cofre tem um diretório próprio dentro
/// dela, com o conjunto completo de arquivos criptografados.
pub const VAULTS_DIR_NAME: &str = "vaults";
/// Índice dos cofres existentes e de qual está selecionado. Não é
/// criptografado: guarda apenas rótulo e data, nunca conteúdo — o mesmo nível
/// de exposição que `openptl.bin`, que já traz salt e verificador em claro.
pub const VAULTS_REGISTRY_FILE_NAME: &str = "vaults.bin";
/// Rótulo do cofre criado ao migrar uma instalação de cofre único.
pub const DEFAULT_VAULT_LABEL: &str = "Pessoal";
/// Limite do rótulo, para o índice não virar depósito de texto.
pub const VAULT_LABEL_MAX_LEN: usize = 60;
/// Arquivo principal de metadados do vault.
pub const OPENPTL_FILE_NAME: &str = "openptl.bin";
/// Arquivo com o payload de perfis criptografado.
pub const PROFILE_FILE_NAME: &str = "profile.bin";
/// Índice criptografado com os ids de host e keychain existentes.
pub const MANIFEST_FILE_NAME: &str = "manifest.bin";
/// Store criptografado de known_hosts, em arquivo próprio.
pub const KNOWN_HOSTS_FILE_NAME: &str = "known_hosts.bin";
/// Notas do usuário, em arquivo próprio pelo mesmo motivo do known_hosts.
pub const NOTES_FILE_NAME: &str = "notes.bin";
/// Estado do log de mutações: relógio lógico, fila de envio e mapa CRDT.
/// Nunca vai para o Drive: é o diário local do dispositivo.
pub const MUTATIONS_FILE_NAME: &str = "mutations.bin";

/// Extensão esperada nos arquivos de payload criptografado.
pub const STORAGE_FILE_EXTENSION: &str = "bin";

/// Versão atual do arquivo de metadados do vault.
pub const CURRENT_STORAGE_VERSION: u32 = 2;
/// Versão atual do esquema do payload criptografado.
pub const CURRENT_PAYLOAD_VERSION: u32 = 2;
/// Versão do formato de lote de mutações trafegado entre dispositivos.
pub const MUTATION_SCHEMA_VERSION: u32 = 1;

/// Chaves auxiliares do keychain usadas pela sincronização.
pub const KEYRING_REFRESH_TOKEN: &str = "google-drive-refresh-token";
pub const KEYRING_USER_EMAIL: &str = "google-user-email";
pub const KEYRING_USER_NAME: &str = "google-user-name";
pub const KEYRING_USER_PICTURE: &str = "google-user-picture";

/// Identificadores do Google Drive usados pelo armazenamento remoto.
pub const DRIVE_FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";
pub const DRIVE_ROOT_FOLDER_NAME: &str = "OpenPtl";
pub const DRIVE_TOP_PARENT_ID: &str = "root";

/// Cabeçalho do cofre remoto: salt e verificador da chave mestre. Não guarda
/// segredo, mas é o que permite um dispositivo novo derivar a mesma chave.
pub const REMOTE_HEADER_FILE_NAME: &str = "header.bin";
/// Prefixo dos snapshots remotos. É o único metadado de nome que expomos ao
/// Drive: sem ele, descobrir o snapshot exigiria baixar a pasta inteira.
pub const REMOTE_SNAPSHOT_PREFIX: &str = "snapshot-";
/// Quantidade de lotes remotos que dispara a compactação num snapshot novo.
pub const REMOTE_COMPACTION_THRESHOLD: usize = 200;

/// Tempo máximo de espera pelo retorno do navegador durante o login.
pub const AUTH_CALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Lista oficial de servidores de autenticação. É consultada a cada login e
/// mesclada com os servidores que o usuário cadastrou localmente.
pub const AUTH_SERVERS_URL: &str =
    "https://raw.githubusercontent.com/UrubuCode/OpenPtl/refs/heads/main/auth-servers.json";
/// Tempo máximo para buscar a lista oficial. Falhar aqui não impede o login:
/// o aplicativo segue com os servidores que já conhece.
pub const AUTH_SERVERS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Manifestos de versão publicados junto com cada release.
pub const RELEASE_MANIFEST_STABLE_URL: &str =
    "https://github.com/UrubuCode/OpenPtl/releases/latest/download/latest.json";
pub const RELEASE_MANIFEST_CANARY_URL: &str =
    "https://github.com/UrubuCode/OpenPtl/releases/download/canary-latest/latest.json";

/// Chave pública minisign das releases. É a mesma que o instalador do Tauri
/// usava: os artefatos já publicados continuam válidos.
pub const RELEASE_PUBLIC_KEY: &str =
    "untrusted comment: minisign public key: 6E304272803B7E81\nRWSBfjuAckIwbu3kj/A7fXPqRAm0U4Vdh6hB//vYmtuMTglvEJrhxqZx\n";

/// Identificação enviada ao consultar as releases.
pub const RELEASE_USER_AGENT: &str = "OpenPtl-Updater";
