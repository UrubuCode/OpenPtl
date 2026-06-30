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

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn set(_service: &str, _key: &str, _value: &str) -> Result<()> {
    Err(anyhow::anyhow!(
        "Keychain do sistema indisponivel nesta plataforma; use senha mestre"
    ))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn get(_service: &str, _key: &str) -> Result<String> {
    Err(anyhow::anyhow!(
        "Keychain do sistema indisponivel nesta plataforma; use senha mestre"
    ))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn delete(_service: &str, _key: &str) {}
