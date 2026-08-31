//! Operações de sincronização expostas à interface.
//!
//! Uma rodada é sempre: baixar o que falta, aplicar localmente, enviar a fila
//! e, se o log tiver crescido demais, compactar. O cofre só é tocado na thread
//! que faz a chamada, nunca dentro da tarefa assíncrona, para não segurar o
//! cadeado durante a rede.

use std::sync::Arc;

use anyhow::{anyhow, Result};

use super::Backend;
use crate::constants::REMOTE_COMPACTION_THRESHOLD;
use crate::libs::models::{AuthServer, SyncLoggedUser};
use crate::libs::sync::{fetch_official_servers, request_sync_cancel, Reporter};
use crate::libs::vault::SyncContext;

impl Backend {
    pub fn sync_reporter(&self) -> Reporter {
        self.sync_reporter.clone()
    }

    pub fn sync_user(&self) -> Option<SyncLoggedUser> {
        self.sync.try_lock().ok()?.logged_user()
    }

    /// Busca a lista oficial de servidores, mescla com a local e abre o
    /// navegador no servidor escolhido.
    ///
    /// A consulta acontece a cada login para que uma troca de endereço ou de
    /// `client_id` chegue sem exigir atualização do aplicativo. Falhar nela
    /// não impede o login: seguimos com o que o cofre já conhece.
    pub fn sync_login(&self) {
        let sync = Arc::clone(&self.sync);
        let vault = Arc::clone(&self.vault);
        let reporter = self.sync_reporter.clone();

        self.runtime.spawn(async move {
            if let Ok(servers) = fetch_official_servers().await {
                if let Ok(mut vault) = vault.lock() {
                    let _ = vault.merge_remote_servers(servers);
                }
            }

            let selected = {
                let Ok(vault) = vault.lock() else {
                    return;
                };
                vault.selected_auth_server().ok()
            };
            let Some(server) = selected else {
                return;
            };

            let outcome = sync
                .lock()
                .await
                .google_login(&reporter, &server.address, server.client_id.clone())
                .await;
            report_failure(&reporter, outcome.err());
        });
    }

    /// Rodada completa: recebe, aplica, envia e compacta.
    pub fn sync_now(&self) {
        let Ok((address, fallbacks)) = self.sync_servers() else {
            return;
        };
        let Ok(context) = self.sync_context() else {
            return;
        };

        let sync = Arc::clone(&self.sync);
        let vault = Arc::clone(&self.vault);
        let reporter = self.sync_reporter.clone();

        self.runtime.spawn(async move {
            let mut manager = sync.lock().await;
            manager.use_servers(address, fallbacks);
            manager.use_vault(context.vault_id.clone());

            let fetched = manager
                .fetch_remote(
                    &reporter,
                    &context.key,
                    &context.seen,
                    context.base_snapshot,
                )
                .await;

            let fetched = match fetched {
                Ok(value) => value,
                Err(error) => return report_failure(&reporter, Some(error)),
            };

            // Aplicar antes de enviar: o que chegou pode alterar o que a fila
            // local ainda vai publicar.
            let pending = {
                let Ok(mut vault) = vault.lock() else {
                    return report_failure(&reporter, Some(anyhow!("vault_indisponivel")));
                };
                if let Err(error) = vault.ingest_remote(&fetched.batches, fetched.snapshot) {
                    return report_failure(&reporter, Some(error));
                }
                match vault.pending_batches() {
                    Ok(batches) => batches,
                    Err(error) => return report_failure(&reporter, Some(error)),
                }
            };

            if !pending.is_empty() {
                match manager
                    .push_batches(&reporter, &context.key, &context.header, &pending)
                    .await
                {
                    Ok(pushed) => {
                        if let Ok(mut vault) = vault.lock() {
                            for (mutation_id, file_id) in pushed {
                                let _ = vault.confirm_pushed(mutation_id, &file_id);
                            }
                        }
                    }
                    Err(error) => return report_failure(&reporter, Some(error)),
                }
            }

            let total_remote = fetched.remote_batch_count + pending.len();
            if total_remote >= REMOTE_COMPACTION_THRESHOLD {
                let snapshot = {
                    let Ok(vault) = vault.lock() else { return };
                    vault.snapshot_for_compaction()
                };
                if let Ok(snapshot) = snapshot {
                    if let Ok(file_id) = manager.compact(&reporter, &context.key, &snapshot).await {
                        if let Ok(mut vault) = vault.lock() {
                            let _ = vault.adopt_compaction(&snapshot, &file_id);
                        }
                    }
                }
            }

            reporter.clear_progress();
        });
    }

    /// Mantidos como atalhos da interface: ambos disparam a rodada completa,
    /// porque com log de mutações enviar sem receber deixaria o aparelho
    /// publicando sobre um estado que ele nem conhece.
    pub fn sync_push(&self) {
        self.sync_now();
    }

    pub fn sync_pull(&self) {
        self.sync_now();
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

    /// Atualiza a lista de servidores a partir do repositório oficial.
    pub fn refresh_auth_servers<F>(&self, on_done: F)
    where
        F: FnOnce(Result<Vec<AuthServer>>) + Send + 'static,
    {
        let vault = Arc::clone(&self.vault);
        self.runtime.spawn(async move {
            let fetched = fetch_official_servers().await;
            let outcome = match fetched {
                Ok(servers) => {
                    let mut guard = match vault.lock() {
                        Ok(guard) => guard,
                        Err(_) => return on_done(Err(anyhow!("vault_indisponivel"))),
                    };
                    guard
                        .merge_remote_servers(servers)
                        .and_then(|()| guard.auth_servers_list())
                }
                Err(error) => Err(error),
            };
            on_done(outcome);
        });
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

    fn sync_context(&self) -> Result<SyncContext> {
        self.vault()?.sync_context()
    }
}

fn report_failure(reporter: &Reporter, error: Option<anyhow::Error>) {
    reporter.clear_progress();
    if let Some(error) = error {
        reporter.progress(&format!("{error}"), 0, 0);
    }
}
