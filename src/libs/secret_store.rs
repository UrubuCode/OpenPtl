// Thin wrapper over the OS keychain. On desktop it uses the `keyring` crate; on
// mobile (Android/iOS) there is no supported keyring backend, so the operations
// fail/no-op and callers must fall back to master-password mode (vault) or
// re-authentication (sync). Centralizing the `keyring` dependency here keeps it
// out of the mobile build entirely (see Cargo.toml target gating).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use anyhow::Context;
use anyhow::Result;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn set(service: &str, key: &str, value: &str) -> Result<()> {
    let entry = keyring::Entry::new(service, key).context("Falha ao preparar keychain")?;
    entry
        .set_password(value)
        .context("Falha ao salvar dado no keychain")
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get(service: &str, key: &str) -> Result<String> {
    let entry = keyring::Entry::new(service, key).context("Falha ao preparar keychain")?;
    entry.get_password().context("Campo ausente no keychain")
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn delete(service: &str, key: &str) {
    if let Ok(entry) = keyring::Entry::new(service, key) {
        let _ = entry.delete_password();
    }
}

// Mobile has no OS keychain backend. Secrets (the Google refresh token + logged
// user fields) are stored in the app-private data directory instead — sandboxed
// per-app on Android/iOS. The vault master key is NEVER stored here: keychain-mode
// vaults are blocked on mobile (see VaultManager::init), so only master-password
// vaults exist and the key stays derived-in-memory. `init_dir` must be called at
// startup with the app data dir before any secret op.
#[cfg(any(target_os = "android", target_os = "ios"))]
use std::path::PathBuf;
#[cfg(any(target_os = "android", target_os = "ios"))]
use std::sync::OnceLock;

#[cfg(any(target_os = "android", target_os = "ios"))]
static STORE_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn init_dir(dir: PathBuf) {
    let _ = STORE_DIR.set(dir);
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn secret_path(service: &str, key: &str) -> Result<PathBuf> {
    let dir = STORE_DIR
        .get()
        .ok_or_else(|| anyhow::anyhow!("Secret store nao inicializado"))?;
    Ok(dir.join(format!("secret_{}_{}", sanitize(service), sanitize(key))))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn set(service: &str, key: &str, value: &str) -> Result<()> {
    let path = secret_path(service, key)?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, value.as_bytes())
        .map_err(|error| anyhow::anyhow!("Falha ao salvar secret: {error}"))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn get(service: &str, key: &str) -> Result<String> {
    let path = secret_path(service, key)?;
    std::fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("Campo ausente"))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn delete(service: &str, key: &str) {
    if let Ok(path) = secret_path(service, key) {
        let _ = std::fs::remove_file(path);
    }
}
