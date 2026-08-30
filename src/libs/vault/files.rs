use super::*;

impl VaultManager {
    pub fn current_key(&self) -> Result<[u8; 32]> {
        self.runtime
            .key
            .ok_or_else(|| anyhow!("Chave do vault indisponivel"))
    }

    pub fn decrypt_manifest_bytes(&self, encrypted_bytes: &[u8]) -> Result<ManifestBinPayload> {
        let key = self.current_key()?;
        let encrypted: EncryptedBin = decode_bin(encrypted_bytes, "manifest.bin invalido")?;
        decrypt_bin_payload(&encrypted, &key, "manifest.bin")
    }

    pub fn read_local_bin_file(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let normalized = normalize_bin_file_name(name)?;
        let path = self.storage_root.join(&normalized);
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            fs::read(&path).with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
        Ok(Some(bytes))
    }

    pub fn write_local_bin_file(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let normalized = normalize_bin_file_name(name)?;
        let path = self.storage_root.join(&normalized);
        fs::write(&path, bytes).with_context(|| format!("Falha ao escrever {}", path.display()))
    }

    pub fn remove_local_bin_file(&self, name: &str) -> Result<()> {
        let normalized = normalize_bin_file_name(name)?;
        let path = self.storage_root.join(&normalized);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Falha ao remover arquivo {}", path.display()))?;
        }
        Ok(())
    }

    pub fn list_local_bin_files(&self) -> Result<Vec<(String, Vec<u8>)>> {
        if !self.storage_root.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&self.storage_root)
            .with_context(|| format!("Falha ao listar {}", self.storage_root.display()))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
            else {
                continue;
            };
            if !is_bin_file_name(&name) {
                continue;
            }
            let bytes = fs::read(&path)
                .with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
            files.push((name, bytes));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }

    pub fn replace_local_files(&mut self, files: &HashMap<String, Vec<u8>>) -> Result<()> {
        self.clear_local_storage()?;
        fs::create_dir_all(&self.storage_root)
            .with_context(|| format!("Falha ao criar {}", self.storage_root.display()))?;

        for (name, bytes) in files {
            let normalized = normalize_bin_file_name(name)?;
            let path = self.storage_root.join(&normalized);
            fs::write(&path, bytes)
                .with_context(|| format!("Falha ao escrever {}", path.display()))?;
        }

        if self.runtime.unlocked {
            self.reload_unlocked_from_disk()?;
        }

        Ok(())
    }

    pub fn clear_local_storage(&self) -> Result<()> {
        if !self.storage_root.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.storage_root)
            .with_context(|| format!("Falha ao listar {}", self.storage_root.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)
                    .with_context(|| format!("Falha ao remover pasta {}", path.display()))?;
            } else {
                fs::remove_file(&path)
                    .with_context(|| format!("Falha ao remover arquivo {}", path.display()))?;
            }
        }
        Ok(())
    }

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
}
