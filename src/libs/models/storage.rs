use super::{AppSettings, AuthServer, SyncMetadata, WindowState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
