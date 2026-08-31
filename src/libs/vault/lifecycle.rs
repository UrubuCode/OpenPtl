use super::*;

impl VaultManager {
    pub fn init(&mut self, password: Option<String>) -> Result<VaultStatus> {
        if self.vault_initialized() {
            return Err(anyhow!("Vault ja foi inicializado"));
        }

        // Mobile has no OS keychain, so keychain-mode (no master password) would put
        // the vault key in an app-private file — a security downgrade. Require a
        // master password on mobile.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        if password
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(anyhow!("Senha mestre obrigatoria neste dispositivo"));
        }

        self.clear_local_storage()?;

        let (key_mode, key, salt) = if let Some(raw_password) = password {
            let pass = raw_password.trim().to_string();
            if pass.len() < 6 {
                return Err(anyhow!("A senha mestre deve ter ao menos 6 caracteres"));
            }

            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);
            let key = derive_key(&pass, &salt)?;
            (KeyMode::Password, key, Some(salt))
        } else {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            persist_keychain_key(&key)?;
            (KeyMode::Keychain, key, None)
        };

        let mut payload = VaultPayload {
            version: CURRENT_PAYLOAD_VERSION,
            ..VaultPayload::default()
        };
        ensure_default_server(&mut payload.auth_servers);

        self.runtime = VaultRuntime {
            unlocked: true,
            key_mode: Some(key_mode),
            key: Some(key),
            salt,
            payload: Some(payload),
            created_at: Some(Utc::now().timestamp()),
            materializing: false,
        };

        self.persist()?;
        self.apply_known_hosts_after_load()?;
        self.status()
    }

    pub fn unlock(&mut self, password: Option<String>) -> Result<VaultStatus> {
        if !self.vault_initialized() {
            return Err(anyhow!("Vault ainda nao foi inicializado"));
        }

        let openptl = self.read_openptl_file()?;
        if openptl.version != CURRENT_STORAGE_VERSION {
            return Err(anyhow!(
                "Versao de openptl.bin nao suportada. Atual: {}, encontrada: {}",
                CURRENT_STORAGE_VERSION,
                openptl.version
            ));
        }

        let (key, salt) = match openptl.key_mode {
            KeyMode::Password => {
                let raw_password =
                    password.ok_or_else(|| anyhow!("Senha mestre obrigatoria para este vault"))?;
                let pass = raw_password.trim();
                if pass.is_empty() {
                    return Err(anyhow!("Senha mestre obrigatoria para este vault"));
                }
                let salt = openptl
                    .salt
                    .ok_or_else(|| anyhow!("Salt ausente no openptl.bin"))?;
                let key = derive_key(pass, &salt)?;
                (key, Some(salt))
            }
            KeyMode::Keychain => (load_keychain_key()?, None),
        };

        if compute_key_check(&key) != openptl.key_check {
            return Err(anyhow!("Senha mestre invalida"));
        }

        let mut payload = self.read_payload_from_disk(&key)?;
        payload.version = CURRENT_PAYLOAD_VERSION;
        ensure_default_server(&mut payload.auth_servers);

        self.runtime = VaultRuntime {
            unlocked: true,
            key_mode: Some(openptl.key_mode),
            key: Some(key),
            salt,
            payload: Some(payload),
            created_at: Some(openptl.created_at),
            materializing: false,
        };

        self.apply_known_hosts_after_load()?;
        self.status()
    }

    /// Cria um cofre vazio já alinhado com o cofre remoto.
    ///
    /// Um aparelho novo precisa do mesmo salt para derivar a mesma chave —
    /// sem isso ele não abriria nenhum lote de mutações. O conteúdo chega
    /// depois, pelo log; aqui só nasce o cofre local.
    pub fn init_from_remote_header(
        &mut self,
        password: &str,
        header: &crate::libs::mutations::RemoteHeader,
    ) -> Result<VaultStatus> {
        let salt = header
            .salt
            .ok_or_else(|| anyhow!("Cofre remoto usa chave do sistema e exige senha mestre"))?;
        let key = derive_key(password.trim(), &salt)?;
        if compute_key_check(&key) != header.key_check {
            return Err(anyhow!("Senha mestre invalida para o cofre remoto"));
        }

        self.clear_local_storage()?;

        let mut payload = VaultPayload {
            version: CURRENT_PAYLOAD_VERSION,
            ..VaultPayload::default()
        };
        ensure_default_server(&mut payload.auth_servers);

        self.runtime = VaultRuntime {
            unlocked: true,
            key_mode: Some(KeyMode::Password),
            key: Some(key),
            salt: Some(salt),
            payload: Some(payload),
            created_at: Some(header.created_at),
            // Nada de gerar mutações para o cofre vazio: elas apagariam no
            // Drive tudo o que ainda vai ser baixado.
            materializing: true,
        };

        self.persist()?;
        self.runtime.materializing = false;
        self.apply_known_hosts_after_load()?;
        self.status()
    }

    pub fn lock(&mut self) -> VaultStatus {
        self.runtime = VaultRuntime::default();
        VaultStatus {
            initialized: self.vault_initialized(),
            locked: true,
            key_mode: None,
            recoverable: self.openptl_exists() && !self.vault_initialized(),
        }
    }

    pub fn reset_all(&mut self) -> Result<VaultStatus> {
        self.runtime = VaultRuntime::default();
        self.clear_local_storage()?;
        clear_keychain_key();
        self.status()
    }

    pub fn verify_master_password(&self, password: &str) -> Result<()> {
        self.assert_unlocked()?;

        let normalized = password.trim();
        if normalized.is_empty() {
            return Err(anyhow!("Informe a senha mestre atual"));
        }

        let current_mode = self
            .runtime
            .key_mode
            .clone()
            .ok_or_else(|| anyhow!("Modo de chave nao encontrado"))?;

        match current_mode {
            KeyMode::Password => {
                let salt = self
                    .runtime
                    .salt
                    .ok_or_else(|| anyhow!("Salt ausente para validar senha atual"))?;
                let derived = derive_key(normalized, &salt)?;
                let current_key = self
                    .runtime
                    .key
                    .ok_or_else(|| anyhow!("Chave atual indisponivel"))?;

                if derived != current_key {
                    return Err(anyhow!("Senha mestre atual invalida"));
                }
            }
            KeyMode::Keychain => {
                return Err(anyhow!(
                    "Este vault usa chave do sistema e nao aceita senha mestre local"
                ));
            }
        }

        Ok(())
    }

    pub fn change_master_password(
        &mut self,
        old_password: Option<String>,
        new_password: String,
    ) -> Result<VaultStatus> {
        self.assert_unlocked()?;

        let normalized_new = new_password.trim();
        if normalized_new.len() < 6 {
            return Err(anyhow!(
                "A nova senha mestre deve ter pelo menos 6 caracteres"
            ));
        }

        let current_mode = self
            .runtime
            .key_mode
            .clone()
            .ok_or_else(|| anyhow!("Modo de chave nao encontrado"))?;

        match current_mode {
            KeyMode::Password => {
                let old = old_password
                    .ok_or_else(|| anyhow!("Informe a senha mestre atual para trocar"))?
                    .trim()
                    .to_string();
                if old.is_empty() {
                    return Err(anyhow!("Informe a senha mestre atual para trocar"));
                }
                self.verify_master_password(&old)?;
            }
            KeyMode::Keychain => {}
        }

        let mut new_salt = [0u8; 16];
        OsRng.fill_bytes(&mut new_salt);
        let new_key = derive_key(normalized_new, &new_salt)?;

        self.runtime.key_mode = Some(KeyMode::Password);
        self.runtime.key = Some(new_key);
        self.runtime.salt = Some(new_salt);
        clear_keychain_key();

        self.persist()?;
        self.status()
    }
}
