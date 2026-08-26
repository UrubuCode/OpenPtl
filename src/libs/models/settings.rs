use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EditorPreference {
    #[default]
    Internal,
    Vscode,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModifiedUploadPolicy {
    Auto,
    #[default]
    Ask,
    Manual,
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
