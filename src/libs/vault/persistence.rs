use super::*;

impl VaultManager {
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

    pub(super) fn persist(&mut self) -> Result<()> {
        self.assert_unlocked()?;

        let key_mode = self
            .runtime
            .key_mode
            .clone()
            .ok_or_else(|| anyhow!("Modo de chave nao definido"))?;
        let key = self
            .runtime
            .key
            .ok_or_else(|| anyhow!("Chave do vault indisponivel"))?;

        fs::create_dir_all(&self.storage_root)
            .with_context(|| format!("Falha ao criar {}", self.storage_root.display()))?;

        let now = Utc::now().timestamp();
        let created_at = self.runtime.created_at.unwrap_or(now);

        let payload = self
            .runtime
            .payload
            .as_mut()
            .ok_or_else(|| anyhow!("Payload do vault indisponivel"))?;

        for profile in &mut payload.connections {
            if profile.id.trim().is_empty() {
                profile.id = uuid::Uuid::new_v4().to_string();
            }
            ensure_uuid(&profile.id, "host")?;
            profile.normalize_protocols();
        }

        for entry in &mut payload.keychain {
            if entry.id.trim().is_empty() {
                entry.id = uuid::Uuid::new_v4().to_string();
                entry.created_at = now;
            }
            ensure_uuid(&entry.id, "keychain")?;
        }

        payload.connections.sort_by(|a, b| a.name.cmp(&b.name));
        payload.keychain.sort_by(|a, b| a.name.cmp(&b.name));
        ensure_default_server(&mut payload.auth_servers);

        let profile_payload = ProfileBinPayload {
            version: CURRENT_PAYLOAD_VERSION,
            settings: payload.settings.clone(),
            sync: payload.sync.clone(),
            auth_servers: payload.auth_servers.clone(),
            window_state: payload.window_state.clone(),
        };

        let mut hosts = BTreeSet::new();
        let mut keychain = BTreeSet::new();
        let mut expected_files = HashSet::new();

        for profile in &payload.connections {
            let file_name = format!("{}.bin", profile.id);
            let path = self.storage_root.join(&file_name);
            let encrypted = encrypt_bin_payload(profile, &key, now)?;
            let encoded = encode_bin(&encrypted)?;
            fs::write(&path, &encoded)
                .with_context(|| format!("Falha ao escrever arquivo {}", path.display()))?;
            hosts.insert(profile.id.clone());
            expected_files.insert(file_name);
        }

        for entry in &payload.keychain {
            let file_name = format!("{}.bin", entry.id);
            let path = self.storage_root.join(&file_name);
            let encrypted = encrypt_bin_payload(entry, &key, now)?;
            let encoded = encode_bin(&encrypted)?;
            fs::write(&path, &encoded)
                .with_context(|| format!("Falha ao escrever arquivo {}", path.display()))?;
            keychain.insert(entry.id.clone());
            expected_files.insert(file_name);
        }

        let manifest_payload = ManifestBinPayload {
            version: CURRENT_PAYLOAD_VERSION,
            hosts,
            keychain,
        };
        let manifest_encrypted = encrypt_bin_payload(&manifest_payload, &key, now)?;
        write_bin_file(&self.manifest_path, &manifest_encrypted)?;

        let profile_encrypted = encrypt_bin_payload(&profile_payload, &key, now)?;
        write_bin_file(&self.profile_path, &profile_encrypted)?;

        let openptl = OpenPtlBin {
            version: CURRENT_STORAGE_VERSION,
            key_mode,
            salt: self.runtime.salt,
            key_check: compute_key_check(&key),
            created_at,
            updated_at: now,
        };
        write_bin_file(&self.openptl_path, &openptl)?;

        self.runtime.created_at = Some(created_at);
        self.cleanup_stale_item_files(&expected_files)?;
        // O log é alimentado depois de o cofre estar gravado: um lote só pode
        // descrever um estado que já sobreviveu ao disco.
        self.capture_mutations()
    }

    pub(super) fn cleanup_stale_item_files(&self, expected: &HashSet<String>) -> Result<()> {
        let entries = fs::read_dir(&self.storage_root)
            .with_context(|| format!("Falha ao listar {}", self.storage_root.display()))?;

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
            if name == OPENPTL_FILE_NAME
                || name == PROFILE_FILE_NAME
                || name == MANIFEST_FILE_NAME
                || name == KNOWN_HOSTS_FILE_NAME
                || name == NOTES_FILE_NAME
                || name == MUTATIONS_FILE_NAME
            {
                continue;
            }
            if expected.contains(&name) {
                continue;
            }

            fs::remove_file(&path)
                .with_context(|| format!("Falha ao remover arquivo obsoleto {}", path.display()))?;
        }

        Ok(())
    }

    pub(super) fn read_openptl_file(&self) -> Result<OpenPtlBin> {
        read_bin_file(&self.openptl_path)
    }

    pub(super) fn read_payload_from_disk(&self, key: &[u8; 32]) -> Result<VaultPayload> {
        let profile_encrypted: EncryptedBin = read_bin_file(&self.profile_path)?;
        let profile_payload: ProfileBinPayload =
            decrypt_bin_payload(&profile_encrypted, key, PROFILE_FILE_NAME)?;

        let manifest_encrypted: EncryptedBin = read_bin_file(&self.manifest_path)?;
        let manifest_payload: ManifestBinPayload =
            decrypt_bin_payload(&manifest_encrypted, key, MANIFEST_FILE_NAME)?;

        if profile_payload.version != CURRENT_PAYLOAD_VERSION {
            return Err(anyhow!(
                "Versao de profile.bin nao suportada. Atual: {}, encontrada: {}",
                CURRENT_PAYLOAD_VERSION,
                profile_payload.version
            ));
        }

        if manifest_payload.version != CURRENT_PAYLOAD_VERSION {
            return Err(anyhow!(
                "Versao de manifest.bin nao suportada. Atual: {}, encontrada: {}",
                CURRENT_PAYLOAD_VERSION,
                manifest_payload.version
            ));
        }

        let mut connections = Vec::new();
        let mut keychain = Vec::new();

        for uuid in manifest_payload.hosts {
            ensure_uuid(&uuid, "host")?;
            let path = self.storage_root.join(format!("{}.bin", uuid));
            let encoded = fs::read(&path)
                .with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
            let encrypted: EncryptedBin = decode_bin(&encoded, "Arquivo de host invalido")?;
            let mut profile: ConnectionProfile =
                decrypt_bin_payload(&encrypted, key, "Arquivo de host")?;
            profile.id = uuid;
            profile.normalize_protocols();
            connections.push(profile);
        }

        for uuid in manifest_payload.keychain {
            ensure_uuid(&uuid, "keychain")?;
            let path = self.storage_root.join(format!("{}.bin", uuid));
            let encoded = fs::read(&path)
                .with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
            let encrypted: EncryptedBin = decode_bin(&encoded, "Arquivo de keychain invalido")?;
            let mut entry: KeychainEntry =
                decrypt_bin_payload(&encrypted, key, "Arquivo de keychain")?;
            entry.id = uuid;
            keychain.push(entry);
        }

        Ok(VaultPayload {
            version: CURRENT_PAYLOAD_VERSION,
            connections,
            keychain,
            settings: profile_payload.settings,
            sync: profile_payload.sync,
            auth_servers: profile_payload.auth_servers,
            window_state: profile_payload.window_state,
        })
    }

    pub(super) fn vault_initialized(&self) -> bool {
        self.openptl_exists() && self.profile_path.exists() && self.manifest_path.exists()
    }

    pub(super) fn openptl_exists(&self) -> bool {
        self.openptl_path.exists()
    }
}
