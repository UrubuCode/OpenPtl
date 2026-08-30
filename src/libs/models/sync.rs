use serde::{Deserialize, Serialize};

use super::base::BackendMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub connected: bool,
    pub status: String,
    pub message: BackendMessage,
    pub last_sync_at: Option<String>,
    pub pending_user_code: Option<String>,
    pub verification_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncLoggedUser {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub picture_url: Option<String>,
}

impl SyncState {
    pub fn idle(message: impl Into<BackendMessage>) -> Self {
        Self {
            connected: false,
            status: "idle".to_string(),
            message: message.into(),
            last_sync_at: None,
            pending_user_code: None,
            verification_url: None,
        }
    }

    pub fn ok(message: impl Into<BackendMessage>, last_sync_at: Option<String>) -> Self {
        Self {
            connected: true,
            status: "ok".to_string(),
            message: message.into(),
            last_sync_at,
            pending_user_code: None,
            verification_url: None,
        }
    }

    pub fn error(message: impl Into<BackendMessage>) -> Self {
        Self {
            connected: false,
            status: "error".to_string(),
            message: message.into(),
            last_sync_at: None,
            pending_user_code: None,
            verification_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncConflictKind {
    Host,
    Keychain,
    Profile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflictItem {
    pub kind: SyncConflictKind,
    pub id: String,
    pub label: String,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncConflictPreview {
    #[serde(default)]
    pub conflicts: Vec<SyncConflictItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncKeepSide {
    Client,
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflictDecision {
    pub kind: SyncConflictKind,
    pub id: String,
    pub keep: SyncKeepSide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProbeResult {
    pub found: bool,
    pub message: BackendMessage,
}
