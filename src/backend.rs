use std::path::Path;

use anyhow::{Context, Result};
use tokio::runtime::{Builder, Runtime};

use crate::libs::{
    models::{
        AppSettings, ConnectionProfile, KeychainEntry, SshConnectPurpose, SshConnectResult,
        SshSessionInfo, VaultStatus,
    },
    vault::VaultManager,
};
use crate::protocols::ssh::{
    known_hosts_add, known_hosts_ensure, known_hosts_list, known_hosts_remove, SshManager,
};

pub struct Backend {
    runtime: Runtime,
    pub vault: VaultManager,
    ssh: SshManager,
}

impl Backend {
    pub fn new() -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("Falha ao inicializar runtime assíncrono")?;
        let vault = VaultManager::new().context("Falha ao preparar armazenamento local")?;
        Ok(Self {
            runtime,
            vault,
            ssh: SshManager::new(),
        })
    }

    pub fn status(&self) -> Result<VaultStatus> {
        self.vault.status()
    }

    pub fn initialize(&mut self, password: String) -> Result<VaultStatus> {
        let password = (!password.trim().is_empty()).then_some(password);
        self.vault
            .init(password)
            .context("Falha ao inicializar vault")
    }

    pub fn unlock(&mut self, password: String) -> Result<VaultStatus> {
        self.vault
            .unlock(Some(password))
            .context("Falha ao desbloquear vault")
    }

    pub fn lock(&mut self) -> Result<VaultStatus> {
        for session in self.ssh.list_sessions() {
            self.runtime
                .block_on(self.ssh.disconnect(&session.session_id));
        }
        self.vault.capture_known_hosts()?;
        Ok(self.vault.lock())
    }

    pub fn reset(&mut self) -> Result<VaultStatus> {
        self.vault.reset_all().context("Falha ao resetar vault")
    }

    pub fn connections(&self) -> Result<Vec<ConnectionProfile>> {
        self.vault.connections_list()
    }

    pub fn save_connection(&mut self, profile: ConnectionProfile) -> Result<ConnectionProfile> {
        self.vault
            .connection_save(profile)
            .context("Falha ao salvar conexão")
    }

    pub fn delete_connection(&mut self, id: &str) -> Result<()> {
        self.vault
            .connection_delete(id)
            .context("Falha ao excluir conexão")
    }

    pub fn keychain(&self) -> Result<Vec<KeychainEntry>> {
        self.vault.keychain_list()
    }

    pub fn save_keychain(&mut self, entry: KeychainEntry) -> Result<KeychainEntry> {
        self.vault
            .keychain_save(entry)
            .context("Falha ao salvar credencial")
    }

    pub fn delete_keychain(&mut self, id: &str) -> Result<()> {
        self.vault
            .keychain_delete(id)
            .context("Falha ao excluir credencial")
    }

    pub fn settings(&self) -> Result<AppSettings> {
        self.vault.settings_get()
    }

    pub fn update_settings(&mut self, settings: AppSettings) -> Result<AppSettings> {
        self.vault
            .settings_update(settings)
            .context("Falha ao salvar configurações")
    }

    pub fn connect(
        &mut self,
        profile_id: &str,
        purpose: SshConnectPurpose,
    ) -> Result<SshConnectResult> {
        let profile = self.vault.profile_by_id(profile_id)?;
        let known_hosts_path = self.vault.known_hosts_path();
        let result = self.runtime.block_on(self.ssh.connect_ex(
            &profile,
            Some(known_hosts_path.as_path()),
            false,
            purpose,
            false,
        ))?;
        self.vault.capture_known_hosts()?;
        Ok(result)
    }

    pub fn accept_and_connect(
        &mut self,
        profile_id: &str,
        purpose: SshConnectPurpose,
    ) -> Result<SshConnectResult> {
        let profile = self.vault.profile_by_id(profile_id)?;
        let known_hosts_path = self.vault.known_hosts_path();
        let result = self.runtime.block_on(self.ssh.connect_ex(
            &profile,
            Some(known_hosts_path.as_path()),
            true,
            purpose,
            false,
        ))?;
        self.vault.capture_known_hosts()?;
        Ok(result)
    }

    pub fn connect_local(&mut self, path: Option<&Path>) -> Result<SshSessionInfo> {
        self.ssh
            .connect_local(path)
            .context("Falha ao abrir shell local")
    }

    pub fn sessions(&self) -> Vec<SshSessionInfo> {
        self.ssh.list_sessions()
    }

    pub fn terminal_command(&mut self, session_id: &str, command: &str) -> Result<String> {
        self.runtime
            .block_on(self.ssh.run_command(session_id, command))
            .context("Falha ao executar comando no terminal")
    }

    pub fn terminal_input(&mut self, session_id: &str, input: &str) -> Result<()> {
        self.runtime
            .block_on(self.ssh.write_raw_input(session_id, input.as_bytes()))
            .context("Falha ao enviar entrada ao terminal")
    }

    pub fn disconnect(&mut self, session_id: &str) {
        self.runtime.block_on(self.ssh.disconnect(session_id));
    }

    pub fn known_hosts(&self) -> Result<Vec<crate::libs::models::KnownHostEntry>> {
        known_hosts_list(Some(
            self.vault.known_hosts_path().to_string_lossy().as_ref(),
        ))
        .context("Falha ao listar hosts conhecidos")
    }

    pub fn ensure_known_hosts(&self) -> Result<String> {
        known_hosts_ensure(Some(
            self.vault.known_hosts_path().to_string_lossy().as_ref(),
        ))
        .context("Falha ao preparar known_hosts")
    }

    pub fn add_known_host(
        &mut self,
        host: &str,
        port: u16,
        key_type: &str,
        key_base64: &str,
    ) -> Result<()> {
        known_hosts_add(
            Some(self.vault.known_hosts_path().to_string_lossy().as_ref()),
            host,
            port,
            key_type,
            key_base64,
        )?;
        self.vault.capture_known_hosts()
    }

    pub fn remove_known_host(&mut self, line_raw: &str) -> Result<()> {
        known_hosts_remove(
            Some(self.vault.known_hosts_path().to_string_lossy().as_ref()),
            line_raw,
        )?;
        self.vault.capture_known_hosts()
    }
}
