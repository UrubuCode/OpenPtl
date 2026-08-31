use super::*;

impl VaultManager {
    /// Working known_hosts file materialized from the vault. Lives under the app
    /// data dir (no HOME dependency), so it works on Android. russh verifies host
    /// keys against this file; its content is mirrored into the encrypted vault.
    pub fn known_hosts_path(&self) -> PathBuf {
        self.known_hosts_path.clone()
    }

    pub(super) fn write_known_hosts_file(&self, content: &str) -> Result<()> {
        if let Some(parent) = self.known_hosts_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&self.known_hosts_path, content.as_bytes()).with_context(|| {
            format!(
                "Falha ao escrever known_hosts em {}",
                self.known_hosts_path.display()
            )
        })
    }

    pub(super) fn read_known_hosts_store(&self) -> Result<String> {
        if !self.known_hosts_bin_path.exists() {
            return Ok(String::new());
        }
        let key = self.current_key()?;
        let encrypted: EncryptedBin = read_bin_file(&self.known_hosts_bin_path)?;
        decrypt_bin_payload(&encrypted, &key, KNOWN_HOSTS_FILE_NAME)
    }

    pub(super) fn write_known_hosts_store(&self, content: &str) -> Result<()> {
        let key = self.current_key()?;
        let encrypted = encrypt_bin_payload(&content.to_string(), &key, Utc::now().timestamp())?;
        write_bin_file(&self.known_hosts_bin_path, &encrypted)
    }

    /// After loading the vault: read the encrypted known_hosts.bin (synced), do a
    /// one-time import of the legacy ~/.ssh/known_hosts (desktop only) if empty,
    /// then materialize the plaintext working file so connections can verify.
    pub(super) fn apply_known_hosts_after_load(&mut self) -> Result<()> {
        let mut content = self.read_known_hosts_store().unwrap_or_default();
        if content.trim().is_empty() {
            if let Some(legacy) = legacy_known_hosts_content() {
                content = legacy;
                self.write_known_hosts_store(&content)?;
            }
        }
        self.write_known_hosts_file(&content)
    }

    /// Read the working known_hosts file back into the encrypted known_hosts.bin so
    /// hosts learned/removed during a session are persisted and synced. No-op if
    /// nothing changed.
    pub fn capture_known_hosts(&mut self) -> Result<()> {
        self.assert_unlocked()?;
        let content = fs::read_to_string(&self.known_hosts_path).unwrap_or_default();
        let current = self.read_known_hosts_store().unwrap_or_default();
        if content == current {
            return Ok(());
        }
        self.write_known_hosts_store(&content)?;
        self.capture_mutations()
    }
}
