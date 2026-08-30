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
    AppSettings, ConnectionProfile, ConnectionProtocol, KeychainEntry, KnownHostEntry, Note,
    SshConnectPurpose, SshConnectResult, VaultStatus,
};
use crate::libs::vault::VaultManager;
use crate::protocols::ssh::{known_hosts_list, known_hosts_remove, SshManager};

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

    pub fn settings(&self) -> Result<AppSettings> {
        self.vault()?.settings_get()
    }

    pub fn settings_update(&self, settings: AppSettings) -> Result<AppSettings> {
        self.vault()?.settings_update(settings)
    }

    pub fn keychain(&self) -> Result<Vec<KeychainEntry>> {
        self.vault()?.keychain_list()
    }

    pub fn keychain_entry(&self, id: &str) -> Result<KeychainEntry> {
        self.vault()?.keychain_by_id(id)
    }

    pub fn keychain_save(&self, entry: KeychainEntry) -> Result<KeychainEntry> {
        self.vault()?.keychain_save(entry)
    }

    pub fn keychain_delete(&self, id: &str) -> Result<()> {
        self.vault()?.keychain_delete(id)
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

    /// Drena a saida pendente da sessao e devolve pelo callback. A interface
    /// chama isto num temporizador; nada bloqueia a thread de desenho.
    pub fn poll_output<F>(&self, session_id: &str, on_output: F)
    where
        F: FnOnce(Result<String>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        self.runtime.spawn(async move {
            let output = ssh.lock().await.drain_output(&session_id);
            on_output(output);
        });
    }

    /// Envia bytes crus para o shell da sessao, sem interpretacao.
    pub fn send_input(&self, session_id: &str, bytes: Vec<u8>) {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        self.runtime.spawn(async move {
            let _ = ssh.lock().await.write_raw_input(&session_id, &bytes).await;
        });
    }

    /// Encerra a sessao e libera o canal remoto.
    pub fn disconnect(&self, session_id: &str) {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        self.runtime.spawn(async move {
            ssh.lock().await.disconnect(&session_id).await;
        });
    }

    /// Hosts confiaveis materializados no arquivo de trabalho do cofre.
    pub fn notes(&self) -> Result<Vec<Note>> {
        self.vault()?.notes_list()
    }

    pub fn note(&self, id: &str) -> Result<Note> {
        self.notes()?
            .into_iter()
            .find(|note| note.id == id)
            .ok_or_else(|| anyhow!("Nota nao encontrada"))
    }

    pub fn note_save(&self, note: Note) -> Result<Note> {
        self.vault()?.note_save(note)
    }

    pub fn note_delete(&self, id: &str) -> Result<()> {
        self.vault()?.note_delete(id)
    }

    pub fn storage_path(&self) -> Result<String> {
        Ok(self.vault()?.storage_path().to_string_lossy().to_string())
    }

    pub fn known_hosts(&self) -> Result<Vec<KnownHostEntry>> {
        let path = self.vault()?.known_hosts_path();
        known_hosts_list(Some(&path.to_string_lossy()))
    }

    /// Remove um host e devolve o arquivo ao armazenamento protegido, para que
    /// a revogacao sobreviva ao proximo desbloqueio.
    pub fn known_host_remove(&self, line_raw: &str) -> Result<()> {
        let path = self.vault()?.known_hosts_path();
        known_hosts_remove(Some(&path.to_string_lossy()), line_raw)?;
        self.capture_known_hosts()
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
