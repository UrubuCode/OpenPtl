use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use chrono::Utc;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use directories::ProjectDirs;
use rand::{rngs::OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::secret_store;
use crate::constants::{
    APP_KEYRING_SERVICE, CURRENT_PAYLOAD_VERSION, CURRENT_STORAGE_VERSION, KEYRING_VAULT_KEY,
    KNOWN_HOSTS_FILE_NAME, MANIFEST_FILE_NAME, OPENPTL_FILE_NAME, PROFILE_FILE_NAME,
    STORAGE_DIR_NAME, STORAGE_FILE_EXTENSION,
};
use crate::libs::models::{
    AppSettings, AuthServer, ConnectionProfile, KeyMode, KeychainEntry, ManifestBinPayload,
    ProfileBinPayload, SyncMetadata, VaultPayload, VaultStatus, WindowState,
};

mod files;
mod known_hosts;
mod lifecycle;
mod persistence;
mod records;

#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
struct VaultRuntime {
    unlocked: bool,
    key_mode: Option<KeyMode>,
    key: Option<[u8; 32]>,
    salt: Option<[u8; 16]>,
    payload: Option<VaultPayload>,
    created_at: Option<i64>,
}

pub struct VaultManager {
    storage_root: PathBuf,
    openptl_path: PathBuf,
    profile_path: PathBuf,
    manifest_path: PathBuf,
    known_hosts_path: PathBuf,
    known_hosts_bin_path: PathBuf,
    runtime: VaultRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenPtlBin {
    version: u32,
    key_mode: KeyMode,
    salt: Option<[u8; 16]>,
    key_check: [u8; 32],
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedBin {
    version: u32,
    nonce: [u8; 24],
    ciphertext: Vec<u8>,
    updated_at: i64,
}
fn legacy_known_hosts_content() -> Option<String> {
    let base = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let path = PathBuf::from(base).join(".ssh").join("known_hosts");
    fs::read_to_string(path)
        .ok()
        .filter(|content| !content.trim().is_empty())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn legacy_known_hosts_content() -> Option<String> {
    None
}

fn cleanup_legacy_layout(data_dir: &Path, storage_root: &Path) -> Result<()> {
    let legacy_vault = data_dir.join("vault.enc.json");
    if legacy_vault.exists() {
        let _ = fs::remove_file(&legacy_vault);
    }

    let legacy_default = storage_root.join("default");
    if legacy_default.exists() && legacy_default.is_dir() {
        let _ = fs::remove_dir_all(&legacy_default);
    }

    if storage_root.exists() {
        let entries = fs::read_dir(storage_root)
            .with_context(|| format!("Falha ao listar {}", storage_root.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
                continue;
            }

            let Some(name) = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
            else {
                continue;
            };
            if is_bin_file_name(&name) {
                continue;
            }
            let _ = fs::remove_file(&path);
        }
    }

    Ok(())
}

fn ensure_default_server(servers: &mut Vec<AuthServer>) {
    if !servers.iter().any(|item| item.id == "default") {
        servers.push(AuthServer::default_server());
    }
}

fn ensure_uuid(value: &str, kind: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value).is_err() {
        return Err(anyhow!("ID de {} invalido: {}", kind, value));
    }
    Ok(())
}

fn derive_key(password: &str, salt: &[u8; 16]) -> Result<[u8; 32]> {
    let params = Params::new(19_456, 3, 1, Some(32))
        .map_err(|error| anyhow!("Falha ao configurar parametros Argon2id: {}", error))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|error| anyhow!("Falha ao derivar chave via Argon2id: {}", error))?;
    Ok(key)
}

fn compute_key_check(key: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(b"openptl-key-check-v1");
    let digest = hasher.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
fn hash_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    to_hex(&digest)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn content_hash_bytes(key: &[u8; 32], file_tag: &str, plaintext: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"openptl-content-hash-v1");
    hasher.update(key);
    hasher.update(file_tag.as_bytes());
    hasher.update(plaintext);
    let digest = hasher.finalize();
    to_hex(&digest)
}

fn content_hash_payload<T: Serialize>(
    payload: &T,
    key: &[u8; 32],
    file_tag: &str,
) -> Result<String> {
    let plaintext = encode_bin(payload)?;
    Ok(content_hash_bytes(key, file_tag, &plaintext))
}

fn profile_hash_payload_input(payload: &ProfileBinPayload) -> ProfileBinPayload {
    let mut normalized = payload.clone();
    // Ignore local sync bookkeeping fields so profile conflicts reflect real user-config changes.
    normalized.sync = SyncMetadata {
        last_sync_at: None,
        last_remote_modified: None,
        last_local_change: 0,
    };
    normalized
}

fn profile_content_hash(payload: &ProfileBinPayload, key: &[u8; 32]) -> Result<String> {
    let normalized = profile_hash_payload_input(payload);
    content_hash_payload(&normalized, key, PROFILE_FILE_NAME)
}

fn encode_bin<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|error| anyhow!("Falha ao serializar binario: {}", error))
}

fn decode_bin<T: DeserializeOwned>(bytes: &[u8], context: &str) -> Result<T> {
    bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map(|(value, _)| value)
        .map_err(|error| anyhow!("{}: {}", context, error))
}

fn read_bin_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read(path).with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
    decode_bin(&raw, &format!("Falha ao decodificar {}", path.display()))
}

fn write_bin_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Falha ao criar diretorio {}", parent.display()))?;
    }
    let data = encode_bin(value)?;
    fs::write(path, data).with_context(|| format!("Falha ao escrever arquivo {}", path.display()))
}

fn derive_nonce(key: &[u8; 32], file_tag: &str, plaintext: &[u8]) -> [u8; 24] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(file_tag.as_bytes());
    hasher.update(plaintext);
    let digest = hasher.finalize();

    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&digest[..24]);
    nonce
}

fn encrypt_bin_payload<T: Serialize>(
    payload: &T,
    key: &[u8; 32],
    file_tag: &str,
    updated_at: i64,
) -> Result<EncryptedBin> {
    let plaintext = encode_bin(payload)?;
    let nonce = derive_nonce(key, file_tag, &plaintext);

    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| anyhow!("Falha ao criptografar payload"))?;

    Ok(EncryptedBin {
        version: CURRENT_STORAGE_VERSION,
        nonce,
        ciphertext,
        updated_at,
    })
}

fn decrypt_bin_payload<T: DeserializeOwned>(
    file: &EncryptedBin,
    key: &[u8; 32],
    context_message: &str,
) -> Result<T> {
    if file.version != CURRENT_STORAGE_VERSION {
        return Err(anyhow!(
            "Versao de arquivo nao suportada. Atual: {}, encontrada: {}",
            CURRENT_STORAGE_VERSION,
            file.version
        ));
    }

    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&file.nonce), file.ciphertext.as_ref())
        .map_err(|_| anyhow!("Falha ao descriptografar {}", context_message))?;

    decode_bin(
        &plaintext,
        &format!("Falha ao decodificar {}", context_message),
    )
}

fn normalize_bin_file_name(input: &str) -> Result<String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(anyhow!("Nome de arquivo vazio"));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(anyhow!("Nome de arquivo invalido"));
    }
    if !is_bin_file_name(value) {
        return Err(anyhow!("Apenas arquivos .bin sao permitidos"));
    }
    Ok(value.to_string())
}

fn is_bin_file_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .ends_with(&format!(".{}", STORAGE_FILE_EXTENSION))
}

fn persist_keychain_key(key: &[u8; 32]) -> Result<()> {
    secret_store::set(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY, &to_hex(key))
}

fn load_keychain_key() -> Result<[u8; 32]> {
    let value = secret_store::get(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY)
        .context("Nao foi possivel ler chave do keychain")?;

    let bytes = hex_to_bytes(&value)?;
    if bytes.len() != 32 {
        return Err(anyhow!("Chave do keychain invalida"));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn clear_keychain_key() {
    secret_store::delete(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY);
}

fn hex_to_bytes(input: &str) -> Result<Vec<u8>> {
    let clean = input.trim();
    if !clean.len().is_multiple_of(2) {
        return Err(anyhow!("Hex invalido"));
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let chunk = std::str::from_utf8(&bytes[i..i + 2]).context("Hex invalido")?;
        let value = u8::from_str_radix(chunk, 16).context("Hex invalido")?;
        out.push(value);
    }
    Ok(out)
}

fn touch_local_change(payload: &mut VaultPayload) {
    payload.sync.last_local_change = Utc::now().timestamp();
}

fn normalize_option(input: Option<String>) -> Option<String> {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
