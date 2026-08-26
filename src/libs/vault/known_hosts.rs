use super::*;

impl VaultManager {
    pub fn validate_password_for_openptl_bytes(
        &self,
        openptl_bytes: &[u8],
        password: &str,
    ) -> Result<bool> {
        let openptl: OpenPtlBin = decode_bin(openptl_bytes, "openptl.bin invalido")?;
        match openptl.key_mode {
            KeyMode::Password => {
                let salt = openptl
                    .salt
                    .ok_or_else(|| anyhow!("Salt ausente no openptl.bin"))?;
                let key = derive_key(password.trim(), &salt)?;
                Ok(compute_key_check(&key) == openptl.key_check)
            }
            KeyMode::Keychain => Err(anyhow!(
                "Backup usa keychain do sistema. Recuperacao remota exige senha mestre"
            )),
        }
    }

    pub fn local_manifest_snapshot(&self) -> Result<ManifestBinPayload> {
        let key = self.current_key()?;
        let encrypted: EncryptedBin = read_bin_file(&self.manifest_path)?;
        decrypt_bin_payload(&encrypted, &key, "manifest.bin")
    }

    pub fn reload_unlocked_from_disk_and_persist(&mut self) -> Result<()> {
        self.reload_unlocked_from_disk()?;
        self.persist()
    }

    pub fn save_window_state(&mut self, next: WindowState) -> Result<()> {
        self.assert_unlocked()?;
        let payload = self.payload_mut()?;
        payload.window_state = Some(next);
        self.persist()
    }

    pub fn window_state(&self) -> Result<Option<WindowState>> {
        Ok(self.payload()?.window_state.clone())
    }

    /// Working known_hosts file materialized from the vault. Lives under the app
    /// data dir (no HOME dependency), so it works on Android. russh verifies host
    /// keys against this file; its content is mirrored into the encrypted vault.
    pub fn known_hosts_path(&self) -> PathBuf {
        self.known_hosts_path.clone()
    }

    fn write_known_hosts_file(&self, content: &str) -> Result<()> {
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

    fn read_known_hosts_store(&self) -> Result<String> {
        if !self.known_hosts_bin_path.exists() {
            return Ok(String::new());
        }
        let key = self.current_key()?;
        let encrypted: EncryptedBin = read_bin_file(&self.known_hosts_bin_path)?;
        decrypt_bin_payload(&encrypted, &key, KNOWN_HOSTS_FILE_NAME)
    }

    fn write_known_hosts_store(&self, content: &str) -> Result<()> {
        let key = self.current_key()?;
        let encrypted = encrypt_bin_payload(
            &content.to_string(),
            &key,
            KNOWN_HOSTS_FILE_NAME,
            Utc::now().timestamp(),
        )?;
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
        self.write_known_hosts_store(&content)
    }

    pub(super) fn reload_unlocked_from_disk(&mut self) -> Result<()> {
        self.assert_unlocked()?;

        let key = self
            .runtime
            .key
            .ok_or_else(|| anyhow!("Chave do vault indisponivel"))?;
        let openptl = self.read_openptl_file()?;

        if compute_key_check(&key) != openptl.key_check {
            return Err(anyhow!("openptl.bin local pertence a outra chave mestre"));
        }

        let mut payload = self.read_payload_from_disk(&key)?;
        payload.version = CURRENT_PAYLOAD_VERSION;
        ensure_default_server(&mut payload.auth_servers);

        self.runtime.payload = Some(payload);
        self.runtime.key_mode = Some(openptl.key_mode);
        self.runtime.salt = openptl.salt;
        self.runtime.created_at = Some(openptl.created_at);

        self.apply_known_hosts_after_load()?;
        Ok(())
    }

    pub(super) fn assert_unlocked(&self) -> Result<()> {
        if !self.runtime.unlocked {
            return Err(anyhow!("Vault bloqueado. Desbloqueie para continuar."));
        }
        Ok(())
    }

    pub(super) fn payload(&self) -> Result<&VaultPayload> {
        self.assert_unlocked()?;
        self.runtime
            .payload
            .as_ref()
            .ok_or_else(|| anyhow!("Payload do vault indisponivel"))
    }

    pub(super) fn payload_mut(&mut self) -> Result<&mut VaultPayload> {
        self.assert_unlocked()?;
        self.runtime
            .payload
            .as_mut()
            .ok_or_else(|| anyhow!("Payload do vault indisponivel"))
    }
}
