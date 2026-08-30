//! Constantes de domínio compartilhadas entre vault, protocolos e apresentação.

/// Serviço usado como namespace no keychain do sistema operacional.
pub const APP_KEYRING_SERVICE: &str = "com.urubucode.openptl";
/// Chave que guarda o material criptográfico do vault no keychain.
pub const KEYRING_VAULT_KEY: &str = "vault-key";

/// Pasta raiz do armazenamento criptografado local.
pub const STORAGE_DIR_NAME: &str = "OpenPtl";
/// Arquivo principal de metadados do vault.
pub const OPENPTL_FILE_NAME: &str = "openptl.bin";
/// Arquivo com o payload de perfis criptografado.
pub const PROFILE_FILE_NAME: &str = "profile.bin";
/// Arquivo de manifesto criptografado.
pub const MANIFEST_FILE_NAME: &str = "manifest.bin";
/// Store criptografado de known_hosts. Fica num .bin separado para não alterar
/// o layout posicional de profile.bin, o que quebraria vaults existentes.
pub const KNOWN_HOSTS_FILE_NAME: &str = "known_hosts.bin";
/// Notas do usuário, em arquivo próprio pelo mesmo motivo do known_hosts.
pub const NOTES_FILE_NAME: &str = "notes.bin";

/// Extensão esperada nos arquivos de payload criptografado.
pub const STORAGE_FILE_EXTENSION: &str = "bin";

/// Versão atual do arquivo de metadados do vault.
pub const CURRENT_STORAGE_VERSION: u32 = 1;
/// Versão atual do esquema do payload criptografado.
pub const CURRENT_PAYLOAD_VERSION: u32 = 1;

/// Chaves auxiliares do keychain usadas pela sincronização.
pub const KEYRING_REFRESH_TOKEN: &str = "google-drive-refresh-token";
pub const KEYRING_USER_EMAIL: &str = "google-user-email";
pub const KEYRING_USER_NAME: &str = "google-user-name";
pub const KEYRING_USER_PICTURE: &str = "google-user-picture";

/// Identificadores do Google Drive usados pelo armazenamento remoto.
pub const DRIVE_FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";
pub const DRIVE_ROOT_FOLDER_NAME: &str = "OpenPtl";
pub const DRIVE_TOP_PARENT_ID: &str = "root";

/// Tempo máximo de espera pelo retorno do navegador durante o login.
pub const AUTH_CALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
