//! Gestão dos cofres e do ciclo de vida do que está aberto.
//!
//! O usuário pode manter vários cofres — pessoal, trabalho, cliente — e cada
//! um é um diretório fechado em si, com senha mestre, log de mutações e pasta
//! remota próprios. Trocar de cofre tranca o anterior antes de abrir o
//! seguinte: deixar a chave de um em memória enquanto outro está em uso
//! misturaria contextos que existem justamente para ficar separados.

use std::path::PathBuf;
use std::sync::MutexGuard;

use anyhow::{anyhow, Result};

use super::Backend;
use crate::constants::DEFAULT_VAULT_LABEL;
use crate::libs::models::{VaultEntry, VaultStatus};
use crate::libs::vault::{VaultManager, VaultRegistry};

impl Backend {
    /// Cofres cadastrados, na ordem em que foram criados.
    pub fn vaults(&self) -> Result<Vec<VaultEntry>> {
        Ok(self.registry()?.list())
    }

    pub fn selected_vault(&self) -> Result<Option<VaultEntry>> {
        Ok(self.registry()?.selected())
    }

    /// Cria um cofre e passa a apontar para ele. O cofre nasce sem senha
    /// mestre: quem define é a tela de abertura, como num aparelho novo.
    pub fn vault_create(&self, label: &str) -> Result<VaultStatus> {
        let path = {
            let mut registry = self.registry()?;
            registry.create(label)?;
            registry.selected_path()?
        };
        self.reopen_vault(path)
    }

    /// Troca o cofre em uso. O anterior é trancado antes: manter a chave de um
    /// cofre em memória enquanto outro está aberto é vazamento entre contextos
    /// que deveriam ser estanques.
    pub fn vault_select(&self, id: &str) -> Result<VaultStatus> {
        let path = {
            let mut registry = self.registry()?;
            registry.select(id)?;
            registry.path_of(id)?
        };
        self.reopen_vault(path)
    }

    pub fn vault_rename(&self, id: &str, label: &str) -> Result<Vec<VaultEntry>> {
        let mut registry = self.registry()?;
        registry.rename(id, label)?;
        Ok(registry.list())
    }

    /// Apaga um cofre e tudo o que ele guarda neste aparelho.
    ///
    /// O conteúdo no Drive continua intacto: apagá-lo daqui destruiria o cofre
    /// para os outros aparelhos, e isso não pode ser efeito colateral de uma
    /// limpeza local.
    pub fn vault_delete(&self, id: &str) -> Result<VaultStatus> {
        let path = {
            let mut registry = self.registry()?;
            registry.remove(id)?;
            if registry.list().is_empty() {
                registry.create(DEFAULT_VAULT_LABEL)?;
            }
            registry.selected_path()?
        };
        self.reopen_vault(path)
    }

    fn reopen_vault(&self, path: PathBuf) -> Result<VaultStatus> {
        let mut guard = self
            .vault
            .lock()
            .map_err(|_| anyhow!("Cofre indisponivel"))?;
        guard.lock();
        *guard = VaultManager::open_at(path)?;
        guard.status()
    }

    fn registry(&self) -> Result<MutexGuard<'_, VaultRegistry>> {
        self.registry
            .lock()
            .map_err(|_| anyhow!("Indice de cofres indisponivel"))
    }

    /// Anota no índice que o cofre selecionado passou a ter senha mestre.
    fn sync_registry_flag(&self, status: &VaultStatus) {
        let Ok(mut registry) = self.registry() else {
            return;
        };
        let Some(id) = registry.selected_id() else {
            return;
        };
        let _ = registry.mark_initialized(&id, status.initialized);
    }

    pub fn status(&self) -> Result<VaultStatus> {
        self.vault()?.status()
    }

    pub fn initialize(&self, password: &str) -> Result<VaultStatus> {
        let status = self.vault()?.init(Some(password.to_owned()))?;
        self.sync_registry_flag(&status);
        Ok(status)
    }

    pub fn unlock(&self, password: &str) -> Result<VaultStatus> {
        let status = self.vault()?.unlock(Some(password.to_owned()))?;
        self.sync_registry_flag(&status);
        Ok(status)
    }

    pub fn lock(&self) -> Result<VaultStatus> {
        Ok(self.vault()?.lock())
    }

    /// Troca a senha mestre. A confirmação é conferida antes de tocar no cofre:
    /// uma divergência aqui evita recriptografar tudo com uma senha digitada
    /// errada, que trancaria o usuário para fora dos próprios dados.
    pub fn change_master_password(
        &self,
        current: &str,
        next: &str,
        confirmation: &str,
    ) -> Result<()> {
        if next != confirmation {
            return Err(anyhow!("A nova senha e a confirmacao nao coincidem"));
        }
        if next.chars().count() < 6 {
            return Err(anyhow!("A nova senha precisa de ao menos 6 caracteres"));
        }

        let mut vault = self.vault()?;
        vault.verify_master_password(current)?;
        vault.change_master_password(Some(current.to_owned()), next.to_owned())?;
        Ok(())
    }
}
