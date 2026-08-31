use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::base::{ConnectionProfile, KeychainEntry};
use super::settings::AppSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthServer {
    pub id: String,
    pub label: String,
    pub address: String,
    pub author: Option<String>,
    #[serde(default)]
    pub official: bool,
    pub client_id: Option<String>,
    #[serde(skip, default)]
    pub from_remote: bool,
}

impl AuthServer {
    pub fn default_server() -> Self {
        Self {
            id: "default".to_string(),
            label: "OpenPtl Official (Cloudflare Worker)".to_string(),
            address: "https://openptl-auth.example.workers.dev".to_string(),
            author: Some("https://github.com/urubucode".to_string()),
            official: true,
            client_id: None,
            from_remote: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    pub last_sync_at: Option<String>,
    pub last_remote_modified: Option<String>,
    pub last_local_change: i64,
}

impl Default for SyncMetadata {
    fn default() -> Self {
        Self {
            last_sync_at: None,
            last_remote_modified: None,
            last_local_change: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultPayload {
    pub version: u32,
    #[serde(default)]
    pub connections: Vec<ConnectionProfile>,
    #[serde(default)]
    pub keychain: Vec<KeychainEntry>,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub sync: SyncMetadata,
    #[serde(default)]
    pub auth_servers: Vec<AuthServer>,
    #[serde(default)]
    pub window_state: Option<WindowState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyMode {
    Password,
    Keychain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub locked: bool,
    pub key_mode: Option<KeyMode>,
    #[serde(default)]
    pub recoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileBinPayload {
    pub version: u32,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub sync: SyncMetadata,
    #[serde(default)]
    pub auth_servers: Vec<AuthServer>,
    #[serde(default)]
    pub window_state: Option<WindowState>,
}

/// Índice dos registros gravados em arquivo próprio.
///
/// Guardava um hash de conteúdo por item para detectar divergência entre
/// dispositivos; o log de mutações passou a resolver isso, e o AEAD de cada
/// arquivo já autentica o conteúdo. Sobrou o que ele sempre foi de fato: a
/// lista de ids que a leitura precisa percorrer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestBinPayload {
    pub version: u32,
    #[serde(default)]
    pub hosts: BTreeSet<String>,
    #[serde(default)]
    pub keychain: BTreeSet<String>,
}
