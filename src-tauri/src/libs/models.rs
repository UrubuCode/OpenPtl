use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    Host,         // 0
    Sftp,         // 1
    LegacyRdp,    // 2 (reserved for bincode compat)
    Both,         // 3
    LegacyVnc,    // 4 (reserved for bincode compat)
}

impl Default for ConnectionKind {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionProtocol {
    Ssh,          // 0
    Sftp,         // 1
    LegacyFtp,    // 2 (reserved for bincode compat)
    LegacyFtps,   // 3 (reserved for bincode compat)
    LegacySmb,    // 4 (reserved for bincode compat)
    LegacyRdp,    // 5 (reserved for bincode compat)
    LegacyVnc,    // 6 (reserved for bincode compat)
}

impl ConnectionProtocol {
    fn is_supported(&self) -> bool {
        matches!(self, ConnectionProtocol::Ssh | ConnectionProtocol::Sftp)
    }
}

impl Default for ConnectionProtocol {
    fn default() -> Self {
        Self::Ssh
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
                ConnectionKind::Both
                | ConnectionKind::LegacyRdp
                | ConnectionKind::LegacyVnc => {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeychainEntryType {
    Password,
    SshKey,
    Secret,
}

impl Default for KeychainEntryType {
    fn default() -> Self {
        Self::Password
    }
}

fn default_keychain_entry_type() -> KeychainEntryType {
    KeychainEntryType::Password
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditorPreference {
    Internal,
    Vscode,
    System,
}

impl Default for EditorPreference {
    fn default() -> Self {
        Self::Internal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModifiedUploadPolicy {
    Auto,
    Ask,
    Manual,
}

impl Default for ModifiedUploadPolicy {
    fn default() -> Self {
        Self::Ask
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub preferred_editor: EditorPreference,
    #[serde(default = "default_external_editor_command")]
    pub external_editor_command: String,
    #[serde(default = "default_sync_auto_enabled")]
    pub sync_auto_enabled: bool,
    #[serde(default = "default_sync_on_startup")]
    pub sync_on_startup: bool,
    #[serde(default = "default_sync_on_settings_change")]
    pub sync_on_settings_change: bool,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_minutes: u32,
    #[serde(default = "default_sftp_chunk_size_kb")]
    pub sftp_chunk_size_kb: u32,
    #[serde(default = "default_sftp_reconnect_delay_seconds")]
    pub sftp_reconnect_delay_seconds: u32,
    #[serde(default = "default_inactivity_lock_minutes")]
    pub inactivity_lock_minutes: u32,
    #[serde(default = "default_auto_reconnect_enabled")]
    pub auto_reconnect_enabled: bool,
    #[serde(default = "default_reconnect_delay_seconds")]
    pub reconnect_delay_seconds: u32,
    #[serde(default = "default_terminal_copy_on_select")]
    pub terminal_copy_on_select: bool,
    #[serde(default = "default_terminal_right_click_paste")]
    pub terminal_right_click_paste: bool,
    #[serde(default = "default_terminal_ctrl_shift_shortcuts")]
    pub terminal_ctrl_shift_shortcuts: bool,
    #[serde(default = "default_debug_logs_enabled")]
    pub debug_logs_enabled: bool,
    #[serde(default)]
    pub modified_files_upload_policy: ModifiedUploadPolicy,
    #[serde(default = "default_known_hosts_path")]
    pub known_hosts_path: String,
    #[serde(default)]
    pub selected_auth_server_id: Option<String>,
}

fn default_external_editor_command() -> String {
    String::new()
}

fn default_sync_auto_enabled() -> bool {
    true
}

fn default_sync_on_startup() -> bool {
    true
}

fn default_sync_on_settings_change() -> bool {
    false
}

fn default_sync_interval() -> u32 {
    5
}

fn default_sftp_chunk_size_kb() -> u32 {
    1024
}

fn default_sftp_reconnect_delay_seconds() -> u32 {
    5
}

fn default_inactivity_lock_minutes() -> u32 {
    10
}

fn default_auto_reconnect_enabled() -> bool {
    true
}

fn default_reconnect_delay_seconds() -> u32 {
    5
}

fn default_terminal_copy_on_select() -> bool {
    true
}

fn default_terminal_right_click_paste() -> bool {
    true
}

fn default_terminal_ctrl_shift_shortcuts() -> bool {
    true
}

fn default_debug_logs_enabled() -> bool {
    false
}

fn default_known_hosts_path() -> String {
    String::new()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            preferred_editor: EditorPreference::Internal,
            external_editor_command: default_external_editor_command(),
            sync_auto_enabled: default_sync_auto_enabled(),
            sync_on_startup: default_sync_on_startup(),
            sync_on_settings_change: default_sync_on_settings_change(),
            sync_interval_minutes: default_sync_interval(),
            sftp_chunk_size_kb: default_sftp_chunk_size_kb(),
            sftp_reconnect_delay_seconds: default_sftp_reconnect_delay_seconds(),
            inactivity_lock_minutes: default_inactivity_lock_minutes(),
            auto_reconnect_enabled: default_auto_reconnect_enabled(),
            reconnect_delay_seconds: default_reconnect_delay_seconds(),
            terminal_copy_on_select: default_terminal_copy_on_select(),
            terminal_right_click_paste: default_terminal_right_click_paste(),
            terminal_ctrl_shift_shortcuts: default_terminal_ctrl_shift_shortcuts(),
            debug_logs_enabled: default_debug_logs_enabled(),
            modified_files_upload_policy: ModifiedUploadPolicy::Ask,
            known_hosts_path: default_known_hosts_path(),
            selected_auth_server_id: None,
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCheckResult {
    pub available: bool,
    pub latest_version: Option<String>,
    pub url: Option<String>,
    pub message: BackendMessage,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestBinPayload {
    pub version: u32,
    pub profile: String,
    #[serde(default)]
    pub hosts: BTreeMap<String, String>,
    #[serde(default)]
    pub keychain: BTreeMap<String, String>,
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use bincode::config::standard;

    // These indices are the on-disk vault format. If any assertion here fails,
    // existing encrypted vaults will fail to decode -> data loss. Do not "fix"
    // a failure by changing the expected index; preserve the enum order instead.
    #[test]
    fn connection_kind_bincode_indices_are_stable() {
        let bytes = bincode::serde::encode_to_vec(ConnectionKind::Both, standard()).unwrap();
        assert_eq!(bytes, vec![3u8], "ConnectionKind::Both must stay at index 3");

        let (both, _): (ConnectionKind, _) =
            bincode::serde::decode_from_slice(&[3u8], standard()).unwrap();
        assert_eq!(both, ConnectionKind::Both);

        let (rdp, _): (ConnectionKind, _) =
            bincode::serde::decode_from_slice(&[2u8], standard()).unwrap();
        assert_eq!(rdp, ConnectionKind::LegacyRdp);

        let (vnc, _): (ConnectionKind, _) =
            bincode::serde::decode_from_slice(&[4u8], standard()).unwrap();
        assert_eq!(vnc, ConnectionKind::LegacyVnc);
    }

    #[test]
    fn connection_protocol_bincode_indices_are_stable() {
        let cases = [
            (ConnectionProtocol::Ssh, 0u8),
            (ConnectionProtocol::Sftp, 1),
            (ConnectionProtocol::LegacyFtp, 2),
            (ConnectionProtocol::LegacyFtps, 3),
            (ConnectionProtocol::LegacySmb, 4),
            (ConnectionProtocol::LegacyRdp, 5),
            (ConnectionProtocol::LegacyVnc, 6),
        ];
        for (variant, idx) in cases {
            let bytes = bincode::serde::encode_to_vec(variant.clone(), standard()).unwrap();
            assert_eq!(bytes, vec![idx], "ConnectionProtocol index drifted for {:?}", variant);
        }
    }

    #[test]
    fn normalize_strips_legacy_protocols() {
        let mut profile = ConnectionProfile {
            protocols: vec![
                ConnectionProtocol::LegacyRdp,
                ConnectionProtocol::Ssh,
                ConnectionProtocol::LegacySmb,
                ConnectionProtocol::Sftp,
            ],
            ..ConnectionProfile::default()
        };
        profile.normalize_protocols();
        assert_eq!(
            profile.protocols,
            vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
        );
    }

    #[test]
    fn normalize_legacy_only_profile_falls_back_to_ssh_sftp() {
        let mut profile = ConnectionProfile {
            protocols: vec![ConnectionProtocol::LegacyRdp],
            ..ConnectionProfile::default()
        };
        profile.normalize_protocols();
        assert_eq!(
            profile.protocols,
            vec![ConnectionProtocol::Ssh, ConnectionProtocol::Sftp]
        );
    }
}
