use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::libs::secret_store;
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

use crate::constants::{
    APP_KEYRING_SERVICE, CURRENT_PAYLOAD_VERSION, CURRENT_STORAGE_VERSION, KEYRING_VAULT_KEY,
    KNOWN_HOSTS_FILE_NAME, MANIFEST_FILE_NAME, OPENPTL_FILE_NAME, PROFILE_FILE_NAME,
    STORAGE_DIR_NAME, STORAGE_FILE_EXTENSION,
};
use crate::libs::models::{
    AppSettings, AuthServer, ConnectionProfile, KeyMode, KeychainEntry, ManifestBinPayload,
    ProfileBinPayload, SyncMetadata, VaultPayload, VaultStatus, WindowState,
};

mod crypto;
mod files;
mod known_hosts;
mod lifecycle;
mod persistence;
mod records;

pub(crate) use crypto::*;

#[derive(Debug, Clone, Default)]
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
pub(crate) struct EncryptedBin {
    version: u32,
    nonce: [u8; 24],
    ciphertext: Vec<u8>,
    updated_at: i64,
}

impl VaultManager {
    // Desktop resolves the data directory itself via ProjectDirs. On Android/iOS
    // ProjectDirs returns None (no XDG/standard dirs), so the caller must supply
    // the OS-provided app data dir from Tauri's path resolver via `new_in`.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "urubucode", "openptl")
            .ok_or_else(|| anyhow!("Nao foi possivel resolver diretorio de dados do aplicativo"))?;
        Self::new_in(dirs.data_dir().to_path_buf())
    }

    pub fn new_in(data_dir: std::path::PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir).with_context(|| {
            format!("Falha ao criar diretorio de dados: {}", data_dir.display())
        })?;

        let storage_root = data_dir.join(STORAGE_DIR_NAME);
        fs::create_dir_all(&storage_root)
            .with_context(|| format!("Falha ao criar diretorio {}", storage_root.display()))?;

        cleanup_legacy_layout(&data_dir, &storage_root)?;

        Ok(Self {
            openptl_path: storage_root.join(OPENPTL_FILE_NAME),
            profile_path: storage_root.join(PROFILE_FILE_NAME),
            manifest_path: storage_root.join(MANIFEST_FILE_NAME),
            known_hosts_path: storage_root.join("known_hosts"),
            known_hosts_bin_path: storage_root.join(KNOWN_HOSTS_FILE_NAME),
            storage_root,
            runtime: VaultRuntime::default(),
        })
    }

    /// Pasta onde os arquivos criptografados do cofre vivem.
    pub fn storage_path(&self) -> PathBuf {
        self.storage_root.clone()
    }

    pub fn status(&self) -> Result<VaultStatus> {
        let initialized = self.vault_initialized();
        let recoverable = self.openptl_exists() && !initialized;

        let key_mode = if self.runtime.key_mode.is_some() {
            self.runtime.key_mode.clone()
        } else if self.openptl_exists() {
            Some(self.read_openptl_file()?.key_mode)
        } else {
            None
        };

        Ok(VaultStatus {
            initialized,
            locked: !self.runtime.unlocked,
            key_mode,
            recoverable,
        })
    }
}

#[cfg(test)]
mod tests;
