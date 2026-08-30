//! Operações de sincronização expostas à interface.
//!
//! Cada uma despacha para o runtime e publica o andamento no relator; a
//! interface lê de lá. O cofre só é tocado na thread que faz a chamada, nunca
//! dentro da tarefa assíncrona, para não segurar o cadeado durante a rede.

use std::sync::Arc;

use anyhow::Result;

use super::Backend;
use crate::libs::models::SyncLoggedUser;
use crate::libs::sync::{request_sync_cancel, Reporter};

impl Backend {
    pub fn sync_reporter(&self) -> Reporter {
        self.sync_reporter.clone()
    }

    pub fn sync_user(&self) -> Option<SyncLoggedUser> {
        self.sync.try_lock().ok()?.logged_user()
    }

    /// Abre o navegador no fluxo do Google e espera o retorno numa porta local.
    pub fn sync_login(&self) {
        let Ok((address, fallbacks)) = self.sync_servers() else {
            return;
        };
        let sync = Arc::clone(&self.sync);
        let reporter = self.sync_reporter.clone();

        self.runtime.spawn(async move {
            let outcome = sync
                .lock()
                .await
                .google_login(&reporter, &address, None)
                .await;
            report_failure(&reporter, outcome.err());
        });
        let _ = fallbacks;
    }

    /// Envia os arquivos do cofre para o Drive.
    pub fn sync_push(&self) {
        let Ok((address, fallbacks)) = self.sync_servers() else {
            return;
        };
        let Ok(files) = self.local_files() else {
            return;
        };

        let sync = Arc::clone(&self.sync);
        let reporter = self.sync_reporter.clone();

        self.runtime.spawn(async move {
            let outcome = sync
                .lock()
                .await
                .push(&reporter, files, &address, &fallbacks)
                .await;
            report_failure(&reporter, outcome.err());
        });
    }

    /// Baixa os arquivos do Drive e substitui o armazenamento local.
    pub fn sync_pull(&self) {
        let Ok((address, fallbacks)) = self.sync_servers() else {
            return;
        };

        let sync = Arc::clone(&self.sync);
        let reporter = self.sync_reporter.clone();
        let vault = Arc::clone(&self.vault);

        self.runtime.spawn(async move {
            let outcome = sync
                .lock()
                .await
                .pull(&reporter, &address, &fallbacks)
                .await;

            match outcome {
                // Só substitui o local quando o remoto trouxe conteúdo: uma
                // resposta vazia não pode apagar o cofre do usuário.
                Ok(Some(files)) => {
                    if let Ok(mut vault) = vault.lock() {
                        report_failure(
                            &reporter,
                            vault
                                .replace_local_files(&files)
                                .and_then(|()| vault.reload_unlocked_from_disk_and_persist())
                                .err(),
                        );
                    }
                }
                Ok(None) => reporter.clear_progress(),
                Err(error) => report_failure(&reporter, Some(error)),
            }
        });
    }

    pub fn sync_cancel(&self) {
        let state = request_sync_cancel();
        self.sync_reporter.status(state);
        self.sync_reporter.clear_progress();
    }

    pub fn sync_logout(&self) {
        if let Ok(sync) = self.sync.try_lock() {
            sync.clear_local_auth();
        }
        self.sync_reporter.clear_progress();
    }

    /// Endereço do servidor de auth escolhido, mais os demais como reserva.
    fn sync_servers(&self) -> Result<(String, Vec<String>)> {
        let vault = self.vault()?;
        let selected = vault.selected_auth_server()?;
        let fallbacks = vault
            .auth_servers_list()?
            .into_iter()
            .map(|server| server.address)
            .filter(|address| *address != selected.address)
            .collect();
        Ok((selected.address, fallbacks))
    }

    fn local_files(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.vault()?.list_local_bin_files()
    }
}

fn report_failure(reporter: &Reporter, error: Option<anyhow::Error>) {
    reporter.clear_progress();
    if let Some(error) = error {
        reporter.progress(&format!("{error}"), 0, 0);
    }
}
