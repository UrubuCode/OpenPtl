use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendMessage {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, String>>,
}

impl BackendMessage {
    pub fn key(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            params: None,
        }
    }

    pub fn with_params(message: impl Into<String>, params: HashMap<String, String>) -> Self {
        Self {
            message: message.into(),
            params: Some(params),
        }
    }
}

impl From<&str> for BackendMessage {
    fn from(value: &str) -> Self {
        Self::key(value)
    }
}

impl From<String> for BackendMessage {
    fn from(value: String) -> Self {
        Self::key(value)
    }
}

impl From<&String> for BackendMessage {
    fn from(value: &String) -> Self {
        Self::key(value.clone())
    }
}

impl std::fmt::Display for BackendMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn default_connection_protocols() -> Vec<ConnectionProtocol> {
    vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
}

fn default_connection_kind() -> Option<ConnectionKind> {
    None
}

// The discriminant ORDER of these enums is part of the on-disk vault format:
// `bincode` encodes enum variants by positional index, not by name. The product
// now only supports SSH/SFTP, but RDP/VNC/FTP/SMB once occupied indices in the
// middle of these enums. Removing them outright would shift the index of every
// later variant (e.g. `Both` went 3 -> 2), making existing encrypted vaults fail
// to decode -> total data loss. The `Legacy*` placeholders keep every original
// index stable so any vault written by an older build still unlocks. They are
// never produced by the UI and are stripped by `normalize_protocols`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    Host,      // 0
    Sftp,      // 1
    LegacyRdp, // 2 (reserved for bincode compat)
    #[default]
    Both, // 3
    LegacyVnc, // 4 (reserved for bincode compat)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionProtocol {
    #[default]
    Ssh, // 0
    Sftp,       // 1
    LegacyFtp,  // 2 (reserved for bincode compat)
    LegacyFtps, // 3 (reserved for bincode compat)
    LegacySmb,  // 4 (reserved for bincode compat)
    LegacyRdp,  // 5 (reserved for bincode compat)
    LegacyVnc,  // 6 (reserved for bincode compat)
}

impl ConnectionProtocol {
    fn is_supported(&self) -> bool {
        matches!(self, ConnectionProtocol::Ssh | ConnectionProtocol::Sftp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub keychain_id: Option<String>,
    pub remote_path: Option<String>,
    #[serde(default = "default_connection_protocols")]
    pub protocols: Vec<ConnectionProtocol>,
    #[serde(default = "default_connection_kind")]
    pub kind: Option<ConnectionKind>,
}

impl ConnectionProfile {
    pub fn normalize_protocols(&mut self) {
        if self.protocols.is_empty() {
            self.protocols = match self.kind.clone().unwrap_or(ConnectionKind::Both) {
                ConnectionKind::Host => vec![ConnectionProtocol::Ssh],
                ConnectionKind::Sftp => vec![ConnectionProtocol::Sftp],
                // Both + legacy RDP/VNC profiles all collapse to the SSH/SFTP product.
                ConnectionKind::Both | ConnectionKind::LegacyRdp | ConnectionKind::LegacyVnc => {
                    vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
                }
            };
        }

        let mut ordered = Vec::new();
        for protocol in &self.protocols {
            // Drop legacy (FTP/FTPS/SMB/RDP/VNC) protocols carried by old vaults.
            if protocol.is_supported() && !ordered.contains(protocol) {
                ordered.push(protocol.clone());
            }
        }
        self.protocols = ordered;

        if self.protocols.is_empty() {
            self.protocols = default_connection_protocols();
        }

        self.kind = None;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeychainEntry {
    pub id: String,
    pub name: String,
    #[serde(default = "default_keychain_entry_type")]
    pub entry_type: KeychainEntryType,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub passphrase: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeychainEntryType {
    #[default]
    Password,
    SshKey,
    Secret,
}

fn default_keychain_entry_type() -> KeychainEntryType {
    KeychainEntryType::Password
}
