//! Fachada única usada pela UI para alcançar o domínio.
//!
//! A camada de apresentação nunca toca `VaultManager`, `SshManager`, arquivos
//! ou tokio diretamente: ela troca ações do usuário por chamadas deste módulo e
//! recebe de volta apenas modelos de domínio.

#![allow(dead_code)]

use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;

use crate::libs::models::{
    ConnectionProfile, ConnectionProtocol, KeychainEntry, SshConnectPurpose, SshConnectResult,
    VaultStatus,
};
use crate::libs::vault::VaultManager;
use crate::protocols::ssh::SshManager;

pub struct Backend {
    vault: Mutex<VaultManager>,
    ssh: Arc<AsyncMutex<SshManager>>,
    runtime: Runtime,
}

impl Backend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            vault: Mutex::new(VaultManager::new()?),
            ssh: Arc::new(AsyncMutex::new(SshManager::new())),
            runtime: Runtime::new()?,
        })
    }

    pub fn status(&self) -> Result<VaultStatus> {
        self.vault()?.status()
    }

    pub fn initialize(&self, password: &str) -> Result<VaultStatus> {
        self.vault()?.init(Some(password.to_owned()))
    }

    pub fn unlock(&self, password: &str) -> Result<VaultStatus> {
        self.vault()?.unlock(Some(password.to_owned()))
    }

    pub fn lock(&self) -> Result<VaultStatus> {
        Ok(self.vault()?.lock())
    }

    pub fn connections(&self) -> Result<Vec<ConnectionProfile>> {
        self.vault()?.connections_list()
    }

    pub fn connection(&self, id: &str) -> Result<ConnectionProfile> {
        self.vault()?.profile_by_id(id)
    }

    pub fn connection_save(&self, profile: ConnectionProfile) -> Result<ConnectionProfile> {
        self.vault()?.connection_save(profile)
    }

    pub fn connection_delete(&self, id: &str) -> Result<()> {
        self.vault()?.connection_delete(id)
    }

    pub fn keychain(&self) -> Result<Vec<KeychainEntry>> {
        self.vault()?.keychain_list()
    }

    /// Abre a sessão fora da thread da interface e devolve o desfecho pelo
    /// callback. Um host desconhecido volta como desafio, nunca como conexão
    /// aceita em silêncio.
    pub fn connect<F>(&self, id: &str, accept_unknown_host: bool, on_result: F) -> Result<()>
    where
        F: FnOnce(Result<SshConnectResult>) + Send + 'static,
    {
        let vault = self.vault()?;
        let profile = vault.profile_by_id(id)?;
        let known_hosts = vault.known_hosts_path();
        drop(vault);

        let purpose = if profile.supports(ConnectionProtocol::Ssh) {
            SshConnectPurpose::Terminal
        } else {
            SshConnectPurpose::Sftp
        };

        let ssh = Arc::clone(&self.ssh);
        self.runtime.spawn(async move {
            let outcome = ssh
                .lock()
                .await
                .connect_ex(
                    &profile,
                    Some(known_hosts.as_path()),
                    accept_unknown_host,
                    purpose,
                )
                .await;
            on_result(outcome);
        });

        Ok(())
    }

    /// Recaptura o known_hosts de trabalho para o armazenamento protegido.
    /// Precisa rodar depois de qualquer aceite de host novo.
    pub fn capture_known_hosts(&self) -> Result<()> {
        self.vault()?.capture_known_hosts()
    }

    fn vault(&self) -> Result<MutexGuard<'_, VaultManager>> {
        self.vault
            .lock()
            .map_err(|_| anyhow!("Estado do cofre ficou inconsistente"))
    }
}
