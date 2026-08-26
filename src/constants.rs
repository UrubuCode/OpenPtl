use std::time::Duration;

pub const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/urubucode/OpenPtl/releases/latest";
pub const RELEASE_CHECK_USER_AGENT: &str = "OpenPtl-Native";
pub const AUTH_SERVERS_REMOTE_URL: &str =
    "https://raw.githubusercontent.com/UrubuCode/OpenPtl/refs/heads/main/auth-servers.json";
pub const AUTH_SERVERS_LOCAL_FALLBACK_JSON: &str = include_str!("../auth-servers.json");
pub const EXTERNAL_EDITOR_TEMP_DIR: &str = "openptl-editor";
pub const DEFAULT_EXTERNAL_FILE_NAME: &str = "openptl-file.txt";
pub const DEFAULT_SSH_KEY_COMMENT: &str = "openptl-generated";
pub const DEFAULT_SFTP_CHUNK_SIZE_KB: u32 = 1024;
pub const MIN_SFTP_CHUNK_SIZE_KB: u32 = 64;
pub const MAX_SFTP_CHUNK_SIZE_KB: u32 = 8192;
pub const DEFAULT_WORKSPACE_WIDTH: f64 = 1440.0;
pub const DEFAULT_WORKSPACE_HEIGHT: f64 = 900.0;
pub const MIN_WORKSPACE_WIDTH: u32 = 480;
pub const MIN_WORKSPACE_HEIGHT: u32 = 800;
pub const DEBUG_LOG_CAPACITY: usize = 2000;
pub const DEFAULT_BINARY_PREVIEW_LIMIT_BYTES: u64 = 25 * 1024 * 1024;
pub const APP_KEYRING_SERVICE: &str = "com.urubucode.openptl";
pub const KEYRING_VAULT_KEY: &str = "vault-key";
pub const KEYRING_REFRESH_TOKEN: &str = "google-drive-refresh-token";
pub const KEYRING_USER_EMAIL: &str = "google-user-email";
pub const KEYRING_USER_NAME: &str = "google-user-name";
pub const KEYRING_USER_PICTURE: &str = "google-user-picture";
pub const STORAGE_DIR_NAME: &str = "OpenPtl";
pub const OPENPTL_FILE_NAME: &str = "openptl.bin";
pub const PROFILE_FILE_NAME: &str = "profile.bin";
pub const MANIFEST_FILE_NAME: &str = "manifest.bin";
pub const KNOWN_HOSTS_FILE_NAME: &str = "known_hosts.bin";
pub const STORAGE_FILE_EXTENSION: &str = "bin";
pub const CURRENT_STORAGE_VERSION: u32 = 1;
pub const CURRENT_PAYLOAD_VERSION: u32 = 1;
pub const DRIVE_FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";
pub const DRIVE_ROOT_FOLDER_NAME: &str = "OpenPtl";
pub const DRIVE_TOP_PARENT_ID: &str = "root";
pub const AUTH_DEEPLINK_TIMEOUT: Duration = Duration::from_secs(300);
