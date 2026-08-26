use super::{AppSettings, BackendMessage, ConnectionProfile, KeychainEntry};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSessionInfo {
    pub session_id: String,
    pub profile_id: String,
    pub connected_at: i64,
    #[serde(default = "default_session_kind")]
    pub session_kind: String,
}

fn default_session_kind() -> String {
    "ssh".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: Option<u32>,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BinaryPreviewResult {
    Ready { base64: String, size: u64 },
    TooLarge { size: u64, limit: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub line_raw: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SshConnectResult {
    Connected {
        session: SshSessionInfo,
    },
    UnknownHostChallenge {
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        known_hosts_path: String,
        message: BackendMessage,
    },
    AuthRequired {
        message: BackendMessage,
    },
    Error {
        message: BackendMessage,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshConnectPurpose {
    Terminal,
    Sftp,
}
