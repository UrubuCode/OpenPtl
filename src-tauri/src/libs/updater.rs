//! Auto-update backend.
//!
//! The update *channel* is a per-device local setting stored OUTSIDE the vault
//! (the vault uses a positional bincode format; adding a field there would
//! corrupt existing vaults). It lives in a tiny JSON file `update.json` in the
//! app data dir: `{"channel":"release"}` or `{"channel":"canary"}`.
//!
//! Desktop uses the Tauri updater plugin (signed `latest.json` bundles).
//! Android/iOS fetch a small JSON manifest and, on Android, download the APK to
//! the cache dir and hand it to the system package installer.
//!
//! ANDROID NOTE: launching the APK install intent requires the manifest
//! permission `<uses-permission android:name="android.permission.REQUEST_INSTALL_PACKAGES"/>`.
//! The AndroidManifest.xml is generated under `gen/android` (gitignored, created
//! by `tauri android init`), so it cannot be edited from here. A CI step / manifest
//! template must inject that permission. Without it the installer intent will be
//! blocked by the OS even though the APK downloads successfully.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Result of an update check. Serialized to the frontend as camelCase.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: String,
    pub notes: String,
}

/// Download progress event payload (`update:progress`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChannelFile {
    channel: String,
}

/// `app_data_dir/update.json`.
pub fn channel_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    dir.join("update.json")
}

/// Reads the configured channel; defaults to `"release"` when absent/invalid.
pub fn get_channel(app: &AppHandle) -> String {
    if let Ok(data) = std::fs::read_to_string(channel_path(app)) {
        if let Ok(cfg) = serde_json::from_str::<ChannelFile>(&data) {
            if cfg.channel.to_lowercase() == "canary" {
                return "canary".to_string();
            }
        }
    }
    "release".to_string()
}

/// Validates and writes the channel to `update.json`.
pub fn set_channel(app: &AppHandle, channel: &str) -> Result<(), String> {
    let normalized = channel.trim().to_lowercase();
    if normalized != "release" && normalized != "canary" {
        return Err("invalid_channel".to_string());
    }
    let path = channel_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&ChannelFile {
        channel: normalized,
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn latest_json_url(channel: &str) -> &'static str {
    if channel == "canary" {
        "https://github.com/UrubuCode/OpenPtl/releases/download/canary-latest/latest.json"
    } else {
        "https://github.com/UrubuCode/OpenPtl/releases/latest/download/latest.json"
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn android_manifest_url(channel: &str) -> &'static str {
    if channel == "canary" {
        "https://github.com/UrubuCode/OpenPtl/releases/download/canary-latest/android-latest.json"
    } else {
        "https://github.com/UrubuCode/OpenPtl/releases/latest/download/android-latest.json"
    }
}

// ---------------------------------------------------------------------------
// Desktop implementation (Tauri updater plugin)
// ---------------------------------------------------------------------------

#[cfg(desktop)]
pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;

    let channel = get_channel(app);
    let endpoint = tauri::Url::parse(latest_json_url(&channel)).map_err(|e| e.to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    let current = app.package_info().version.to_string();

    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => Ok(UpdateInfo {
            available: true,
            current_version: current,
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        }),
        None => Ok(UpdateInfo {
            available: false,
            current_version: current.clone(),
            version: current,
            notes: String::new(),
        }),
    }
}

#[cfg(desktop)]
pub async fn install(app: &AppHandle) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tauri_plugin_updater::UpdaterExt;

    let channel = get_channel(app);
    let endpoint = tauri::Url::parse(latest_json_url(&channel)).map_err(|e| e.to_string())?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no_update_available".to_string())?;

    let downloaded = Arc::new(AtomicU64::new(0));
    let progress_app = app.clone();
    let progress_downloaded = downloaded.clone();

    update
        .download_and_install(
            move |chunk_length, content_length| {
                let total = progress_downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed)
                    + chunk_length as u64;
                let _ = progress_app.emit(
                    "update:progress",
                    ProgressPayload {
                        downloaded: total,
                        total: content_length,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;

    // Relaunch into the freshly installed version. `restart` diverges (`-> !`).
    app.restart();
}

// ---------------------------------------------------------------------------
// Mobile implementation (Android APK download + installer intent)
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Debug, Deserialize)]
struct AndroidManifest {
    version: String,
    #[serde(default)]
    notes: String,
    url: String,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
async fn fetch_android_manifest(app: &AppHandle) -> Result<AndroidManifest, String> {
    let channel = get_channel(app);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(android_manifest_url(&channel))
        .header("User-Agent", "OpenPtl")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http_status_{}", response.status().as_u16()));
    }
    response.json().await.map_err(|e| e.to_string())
}

/// Numeric-component version compare (`1.2.3` style); ignores non-numeric suffixes.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn is_newer(current: &str, candidate: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    }
    let cur = parts(current);
    let cand = parts(candidate);
    for i in 0..cur.len().max(cand.len()) {
        let a = cur.get(i).copied().unwrap_or(0);
        let b = cand.get(i).copied().unwrap_or(0);
        if b > a {
            return true;
        }
        if b < a {
            return false;
        }
    }
    false
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn check(app: &AppHandle) -> Result<UpdateInfo, String> {
    let manifest = fetch_android_manifest(app).await?;
    let current = app.package_info().version.to_string();
    let available = is_newer(&current, &manifest.version);
    Ok(UpdateInfo {
        available,
        current_version: current,
        version: manifest.version,
        notes: manifest.notes,
    })
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn install(app: &AppHandle) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let manifest = fetch_android_manifest(app).await?;

    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let apk_path = cache_dir.join("openptl-update.apk");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(&manifest.url)
        .header("User-Agent", "OpenPtl")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http_status_{}", response.status().as_u16()));
    }

    let total = response.content_length();
    let mut file = std::fs::File::create(&apk_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let _ = app.emit(
            "update:progress",
            ProgressPayload { downloaded, total },
        );
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    // Best-effort hand-off to the system package installer via the opener plugin.
    // On Android this needs REQUEST_INSTALL_PACKAGES in the manifest (see module
    // note). If the intent cannot be launched the APK still sits in the cache dir,
    // so we return Ok rather than failing the whole flow.
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_opener::OpenerExt;
        let _ = app
            .opener()
            .open_path(apk_path.to_string_lossy().to_string(), None::<&str>);
    }

    Ok(())
}
