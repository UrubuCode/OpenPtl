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
/// Extensão esperada nos arquivos de payload criptografado.
pub const STORAGE_FILE_EXTENSION: &str = "bin";

/// Versão atual do arquivo de metadados do vault.
pub const CURRENT_STORAGE_VERSION: u32 = 1;
/// Versão atual do esquema do payload criptografado.
pub const CURRENT_PAYLOAD_VERSION: u32 = 1;
