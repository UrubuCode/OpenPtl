//! Consulta e download de atualizações expostos à interface.
//!
//! O download entrega um instalador verificado; abrir esse instalador é uma
//! ação separada e explícita do usuário. Nada é instalado por conta própria.

use anyhow::{Context, Result};

use super::Backend;
use crate::libs::updater::{read_channel, write_channel, Availability, Channel, Updater};

/// Versão do binário em execução, gravada no build.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

impl Backend {
    /// Consulta o manifesto do canal e devolve o resultado pelo callback.
    pub fn update_check<F>(&self, channel: Channel, on_result: F)
    where
        F: FnOnce(Result<Availability>) + Send + 'static,
    {
        self.runtime.spawn(async move {
            let outcome = match Updater::new(CURRENT_VERSION) {
                Ok(updater) => updater.check(channel).await,
                Err(error) => Err(error),
            };
            on_result(outcome);
        });
    }

    /// Baixa o instalador e verifica a assinatura antes de gravar em disco.
    pub fn update_download<F>(&self, channel: Channel, on_result: F)
    where
        F: FnOnce(Result<std::path::PathBuf>) + Send + 'static,
    {
        let destination = self.update_directory();

        self.runtime.spawn(async move {
            let outcome = match (Updater::new(CURRENT_VERSION), destination) {
                (Ok(updater), Ok(directory)) => updater.download(channel, &directory).await,
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            on_result(outcome);
        });
    }

    /// Entrega o instalador ao sistema. O usuário conclui a instalação ali; um
    /// binário em execução não pode se substituir sozinho com segurança.
    pub fn update_open(&self, installer: &std::path::Path) -> Result<()> {
        open::that_detached(installer)
            .with_context(|| format!("Falha ao abrir {}", installer.display()))
    }

    /// Onde os instaladores baixados ficam: ao lado do cofre, não numa pasta
    /// temporária que o sistema possa limpar no meio do download.
    fn update_directory(&self) -> Result<std::path::PathBuf> {
        Ok(self.vault()?.storage_path().join("updates"))
    }

    /// Canal escolhido, lido do arquivo ao lado do cofre.
    pub fn update_channel(&self) -> Channel {
        self.vault()
            .map(|vault| read_channel(&vault.storage_path()))
            .unwrap_or_default()
    }

    pub fn set_update_channel(&self, channel: Channel) -> Result<()> {
        let directory = self.vault()?.storage_path();
        write_channel(&directory, channel)
    }
}
