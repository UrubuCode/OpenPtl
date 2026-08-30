use serde::{Deserialize, Serialize};

use super::base::BackendMessage;

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
