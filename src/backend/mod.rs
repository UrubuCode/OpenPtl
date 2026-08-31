//! Fachada única usada pela UI para alcançar o domínio.
//!
//! A camada de apresentação nunca toca `VaultManager`, `SshManager`, arquivos
//! ou tokio diretamente: ela troca ações do usuário por chamadas deste módulo e
//! recebe de volta apenas modelos de domínio.
//!
//! `sftp_rename` existe aqui mas ainda não tem gesto na tela de arquivos.
#![allow(dead_code)]

mod sync;
mod update;
mod vaults;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as AsyncMutex;

use crate::constants::DEFAULT_VAULT_LABEL;
use crate::libs::journal::Journal;
use crate::libs::models::{
    AppSettings, AuthServer, ConnectionProfile, ConnectionProtocol, KeychainEntry, KnownHostEntry,
    Note, SftpEntry, SshConnectPurpose, SshConnectResult,
};
use crate::libs::sync::{Reporter as SyncReporter, SyncManager};
use crate::libs::transfer::{Direction, Registry as Transfers};
use crate::libs::vault::{VaultManager, VaultRegistry};
use crate::protocols::ssh::{known_hosts_list, known_hosts_remove, SshManager};

/// Bloco padrão de transferência quando a preferência não pode ser lida.
const DEFAULT_CHUNK_BYTES: usize = 1024 * 1024;

pub struct Backend {
    registry: Arc<Mutex<VaultRegistry>>,
    vault: Arc<Mutex<VaultManager>>,
    ssh: Arc<AsyncMutex<SshManager>>,
    runtime: Runtime,
    transfers: Transfers,
    sync: Arc<AsyncMutex<SyncManager>>,
    sync_reporter: SyncReporter,
    journal: Journal,
}

impl Backend {
    /// Diario de eventos, compartilhado com a interface.
    pub fn journal(&self) -> Journal {
        self.journal.clone()
    }

    pub fn new() -> Result<Self> {
        let mut registry = VaultRegistry::new()?;
        // Sempre existe um cofre selecionado. Um índice vazio só acontece na
        // primeira execução; criar o cofre padrão aqui deixa a tela de abertura
        // com o mesmo fluxo de sempre, pedindo a senha mestre.
        if registry.list().is_empty() {
            registry.create(DEFAULT_VAULT_LABEL)?;
        }
        let vault = VaultManager::open_at(registry.selected_path()?)?;

        Ok(Self {
            registry: Arc::new(Mutex::new(registry)),
            vault: Arc::new(Mutex::new(vault)),
            ssh: Arc::new(AsyncMutex::new(SshManager::new())),
            runtime: Runtime::new()?,
            transfers: Transfers::new(),
            sync: Arc::new(AsyncMutex::new(SyncManager::new())),
            sync_reporter: SyncReporter::new(),
            journal: Journal::new(),
        })
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

    /// Informa ao servidor o novo tamanho da janela do terminal, para que os
    /// programas remotos reflitam a área disponível.
    pub fn resize_pty(&self, session_id: &str, columns: u32, rows: u32) {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        self.runtime.spawn(async move {
            let _ = ssh
                .lock()
                .await
                .resize_pty(&session_id, columns, rows)
                .await;
        });
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

    /// Lista um diretório remoto fora da thread da interface.
    pub fn sftp_list<F>(&self, session_id: &str, path: &str, on_result: F)
    where
        F: FnOnce(Result<Vec<SftpEntry>>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let path = path.to_owned();
        self.runtime.spawn(async move {
            let listing = ssh.lock().await.sftp_list(&session_id, &path).await;
            on_result(listing);
        });
    }

    pub fn sftp_mkdir<F>(&self, session_id: &str, path: &str, on_result: F)
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let path = path.to_owned();
        self.runtime.spawn(async move {
            let outcome = ssh.lock().await.sftp_mkdir(&session_id, &path).await;
            on_result(outcome);
        });
    }

    pub fn sftp_delete<F>(&self, session_id: &str, path: &str, is_dir: bool, on_result: F)
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let path = path.to_owned();
        self.runtime.spawn(async move {
            let outcome = ssh
                .lock()
                .await
                .sftp_delete(&session_id, &path, is_dir)
                .await;
            on_result(outcome);
        });
    }

    pub fn sftp_rename<F>(&self, session_id: &str, from: &str, to: &str, on_result: F)
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let from = from.to_owned();
        let to = to.to_owned();
        self.runtime.spawn(async move {
            let outcome = ssh.lock().await.sftp_rename(&session_id, &from, &to).await;
            on_result(outcome);
        });
    }

    /// Lê um arquivo remoto como texto para edição.
    pub fn sftp_read<F>(&self, session_id: &str, path: &str, on_result: F)
    where
        F: FnOnce(Result<String>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let path = path.to_owned();
        let chunk_size = self.chunk_size();

        self.runtime.spawn(async move {
            let content = ssh
                .lock()
                .await
                .sftp_read(&session_id, &path, chunk_size)
                .await;
            on_result(content);
        });
    }

    /// Grava o conteúdo editado de volta no servidor.
    pub fn sftp_write<F>(&self, session_id: &str, path: &str, content: String, on_result: F)
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        let ssh = Arc::clone(&self.ssh);
        let session_id = session_id.to_owned();
        let path = path.to_owned();
        let chunk_size = self.chunk_size();

        self.runtime.spawn(async move {
            let outcome = ssh
                .lock()
                .await
                .sftp_write(&session_id, &path, &content, chunk_size)
                .await;
            on_result(outcome);
        });
    }

    /// Servidores de autenticação disponíveis.
    pub fn auth_servers(&self) -> Result<Vec<AuthServer>> {
        self.vault()?.auth_servers_list()
    }

    pub fn selected_auth_server(&self) -> Result<AuthServer> {
        self.vault()?.selected_auth_server()
    }

    /// Escolhe por qual servidor a autenticação passa.
    pub fn select_auth_server(&self, id: &str) -> Result<()> {
        let mut settings = self.settings()?;
        settings.selected_auth_server_id = Some(id.to_owned());
        self.settings_update(settings)?;
        Ok(())
    }

    /// Fila de transferências, compartilhada com a interface.
    pub fn transfers(&self) -> Transfers {
        self.transfers.clone()
    }

    /// Baixa um arquivo remoto para o disco local. O progresso é publicado na
    /// fila; a interface lê de lá em vez de receber um evento por bloco.
    pub fn sftp_download(&self, session_id: &str, remote: &str, local: PathBuf) {
        let ssh = Arc::clone(&self.ssh);
        let transfers = self.transfers.clone();
        let session_id = session_id.to_owned();
        let remote = remote.to_owned();
        let chunk_size = self.chunk_size();

        self.runtime.spawn(async move {
            let name = file_name_of(&remote);
            let mut manager = ssh.lock().await;

            let total = manager
                .sftp_file_size(&session_id, &remote)
                .await
                .ok()
                .flatten()
                .unwrap_or(0);
            let id = transfers.start(&name, Direction::Download, total);

            let outcome = match std::fs::File::create(&local) {
                Ok(file) => {
                    let mut writer = std::io::BufWriter::new(file);
                    let progress = transfers.clone();
                    let ticket = id.clone();
                    manager
                        .sftp_download_to_writer(&session_id, &remote, &mut writer, chunk_size, {
                            move |bytes| progress.advance(&ticket, bytes)
                        })
                        .await
                }
                Err(error) => Err(anyhow!("Falha ao criar {}: {error}", local.display())),
            };

            transfers.finish(&id, outcome.map_err(|error| format!("{error}")));
        });
    }

    /// Envia um arquivo local para o servidor.
    pub fn sftp_upload(&self, session_id: &str, local: PathBuf, remote: &str) {
        let ssh = Arc::clone(&self.ssh);
        let transfers = self.transfers.clone();
        let session_id = session_id.to_owned();
        let remote = remote.to_owned();
        let chunk_size = self.chunk_size();

        self.runtime.spawn(async move {
            let name = file_name_of(&remote);
            let total = std::fs::metadata(&local)
                .map(|meta| meta.len())
                .unwrap_or(0);
            let id = transfers.start(&name, Direction::Upload, total);

            let outcome = match std::fs::File::open(&local) {
                Ok(file) => {
                    let mut reader = std::io::BufReader::new(file);
                    let progress = transfers.clone();
                    let ticket = id.clone();
                    ssh.lock()
                        .await
                        .sftp_upload_from_reader(&session_id, &remote, &mut reader, chunk_size, {
                            move |bytes| progress.advance(&ticket, bytes)
                        })
                        .await
                }
                Err(error) => Err(anyhow!("Falha ao abrir {}: {error}", local.display())),
            };

            transfers.finish(&id, outcome.map_err(|error| format!("{error}")));
        });
    }

    /// Tamanho de bloco configurado no cofre; o padrão vale se o cofre estiver
    /// indisponível, para não travar uma transferência por causa disso.
    fn chunk_size(&self) -> usize {
        self.settings()
            .map(|settings| settings.sftp_chunk_size_kb as usize * 1024)
            .unwrap_or(DEFAULT_CHUNK_BYTES)
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

/// Nome exibido na fila: só o último segmento do caminho remoto.
fn file_name_of(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_owned()
}
