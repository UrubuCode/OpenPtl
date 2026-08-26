use super::*;

impl VaultManager {
    pub fn connections_list(&self) -> Result<Vec<ConnectionProfile>> {
        let mut connections = self.payload()?.connections.clone();
        connections
            .iter_mut()
            .for_each(|profile| profile.normalize_protocols());
        Ok(connections)
    }

    pub fn connection_save(&mut self, mut profile: ConnectionProfile) -> Result<ConnectionProfile> {
        self.assert_unlocked()?;

        if profile.id.trim().is_empty() {
            profile.id = uuid::Uuid::new_v4().to_string();
        }

        if uuid::Uuid::parse_str(profile.id.trim()).is_err() {
            return Err(anyhow!("ID de host invalido: deve ser UUID"));
        }

        if profile.port == 0 {
            profile.port = 22;
        }

        profile.host = profile.host.trim().to_string();
        profile.username = profile.username.trim().to_string();
        if profile.name.trim().is_empty() {
            profile.name = profile.host.clone();
        } else {
            profile.name = profile.name.trim().to_string();
        }

        profile.password = normalize_option(profile.password);
        profile.private_key = normalize_option(profile.private_key);
        profile.keychain_id = normalize_option(profile.keychain_id);
        profile.remote_path = normalize_option(profile.remote_path);
        profile.normalize_protocols();

        let payload = self.payload_mut()?;
        payload.connections.retain(|item| item.id != profile.id);
        payload.connections.push(profile.clone());
        payload.connections.sort_by(|a, b| a.name.cmp(&b.name));
        touch_local_change(payload);

        self.persist()?;
        Ok(profile)
    }

    pub fn connection_delete(&mut self, id: &str) -> Result<()> {
        self.assert_unlocked()?;
        let payload = self.payload_mut()?;
        payload.connections.retain(|item| item.id != id);
        touch_local_change(payload);
        self.persist()
    }

    pub fn profile_by_id(&self, id: &str) -> Result<ConnectionProfile> {
        let mut profile = self
            .payload()?
            .connections
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Perfil {} nao encontrado", id))?;
        profile.normalize_protocols();
        Ok(profile)
    }

    pub fn keychain_by_id(&self, id: &str) -> Result<KeychainEntry> {
        self.payload()?
            .keychain
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Keychain {} nao encontrado", id))
    }

    pub fn keychain_list(&self) -> Result<Vec<KeychainEntry>> {
        Ok(self.payload()?.keychain.clone())
    }

    pub fn keychain_save(&mut self, mut entry: KeychainEntry) -> Result<KeychainEntry> {
        self.assert_unlocked()?;

        if entry.id.trim().is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
            entry.created_at = Utc::now().timestamp();
        }

        if uuid::Uuid::parse_str(entry.id.trim()).is_err() {
            return Err(anyhow!("ID de keychain invalido: deve ser UUID"));
        }

        entry.name = entry.name.trim().to_string();
        entry.password = normalize_option(entry.password);
        entry.private_key = normalize_option(entry.private_key);
        entry.public_key = normalize_option(entry.public_key);
        entry.passphrase = normalize_option(entry.passphrase);

        if entry.name.is_empty() {
            return Err(anyhow!("Nome e obrigatorio no keychain"));
        }

        if entry.private_key.is_none() && entry.public_key.is_none() && entry.password.is_none() {
            return Err(anyhow!(
                "Informe ao menos uma credencial no keychain (senha, chave privada ou chave publica)"
            ));
        }

        let payload = self.payload_mut()?;
        payload.keychain.retain(|item| item.id != entry.id);
        payload.keychain.push(entry.clone());
        payload.keychain.sort_by(|a, b| a.name.cmp(&b.name));
        touch_local_change(payload);
        self.persist()?;

        Ok(entry)
    }

    pub fn keychain_delete(&mut self, id: &str) -> Result<()> {
        self.assert_unlocked()?;
        let payload = self.payload_mut()?;
        payload.keychain.retain(|item| item.id != id);
        touch_local_change(payload);
        self.persist()
    }

    pub fn auth_servers_list(&self) -> Result<Vec<AuthServer>> {
        let mut servers = if self.runtime.unlocked {
            self.payload()?.auth_servers.clone()
        } else {
            Vec::new()
        };
        ensure_default_server(&mut servers);
        servers.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(servers)
    }

    pub fn merge_remote_servers(&mut self, remote: Vec<AuthServer>) -> Result<()> {
        if !self.runtime.unlocked {
            return Ok(());
        }

        let payload = self.payload_mut()?;
        for mut server in remote {
            server.from_remote = true;
            if let Some(existing) = payload.auth_servers.iter_mut().find(|s| s.id == server.id) {
                *existing = server;
            } else {
                payload.auth_servers.push(server);
            }
        }
        ensure_default_server(&mut payload.auth_servers);
        self.persist()
    }

    pub fn auth_server_save(&mut self, mut server: AuthServer) -> Result<AuthServer> {
        self.assert_unlocked()?;

        if server.id.trim().is_empty() {
            server.id = uuid::Uuid::new_v4().to_string();
        }

        server.label = server.label.trim().to_string();
        server.address = server.address.trim().trim_end_matches('/').to_string();

        if server.label.is_empty() {
            return Err(anyhow!("Label e obrigatorio"));
        }
        if server.address.is_empty() {
            return Err(anyhow!("Endereco e obrigatorio"));
        }
        if !server.address.starts_with("http://") && !server.address.starts_with("https://") {
            return Err(anyhow!("Endereco deve comecar com http:// ou https://"));
        }

        let payload = self.payload_mut()?;
        payload.auth_servers.retain(|s| s.id != server.id);
        payload.auth_servers.push(server.clone());
        ensure_default_server(&mut payload.auth_servers);
        payload.auth_servers.sort_by(|a, b| a.label.cmp(&b.label));
        touch_local_change(payload);
        self.persist()?;

        Ok(server)
    }

    pub fn auth_server_delete(&mut self, id: &str) -> Result<()> {
        self.assert_unlocked()?;
        if id == "default" {
            return Err(anyhow!("Nao e possivel remover o servidor padrao"));
        }
        let is_remote = self
            .payload()?
            .auth_servers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.from_remote)
            .unwrap_or(false);
        if is_remote {
            return Err(anyhow!("Nao e possivel remover servidor remoto"));
        }
        let payload = self.payload_mut()?;
        payload.auth_servers.retain(|s| s.id != id);
        if payload.settings.selected_auth_server_id.as_deref() == Some(id) {
            payload.settings.selected_auth_server_id = None;
        }
        ensure_default_server(&mut payload.auth_servers);
        touch_local_change(payload);
        self.persist()
    }

    pub fn selected_auth_server(&self) -> Result<AuthServer> {
        if !self.runtime.unlocked {
            return Ok(AuthServer::default_server());
        }

        let payload = self.payload()?;
        let selected_id = payload
            .settings
            .selected_auth_server_id
            .as_deref()
            .unwrap_or("default");
        payload
            .auth_servers
            .iter()
            .find(|s| s.id == selected_id)
            .cloned()
            .or_else(|| Some(AuthServer::default_server()))
            .ok_or_else(|| anyhow!("Servidor de auth nao encontrado"))
    }

    pub fn settings_get(&self) -> Result<AppSettings> {
        Ok(self.payload()?.settings.clone())
    }

    pub fn settings_update(&mut self, mut settings: AppSettings) -> Result<AppSettings> {
        self.assert_unlocked()?;
        settings.external_editor_command = settings.external_editor_command.trim().to_string();
        settings.known_hosts_path = settings.known_hosts_path.trim().to_string();
        settings.sync_interval_minutes = settings.sync_interval_minutes.clamp(1, 60);
        settings.sftp_chunk_size_kb = settings.sftp_chunk_size_kb.clamp(64, 8192);
        settings.sftp_reconnect_delay_seconds = settings.sftp_reconnect_delay_seconds.clamp(1, 120);
        settings.inactivity_lock_minutes = settings.inactivity_lock_minutes.clamp(1, 240);
        settings.reconnect_delay_seconds = settings.reconnect_delay_seconds.clamp(1, 120);

        let payload = self.payload_mut()?;
        payload.settings = settings.clone();
        touch_local_change(payload);
        self.persist()?;

        Ok(settings)
    }
}
