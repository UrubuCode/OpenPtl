//! Fachada única usada pela UI para alcançar o domínio.
//!
//! A camada de apresentação nunca toca `VaultManager`, arquivos ou protocolos
//! diretamente: ela troca ações do usuário por chamadas deste módulo e recebe
//! de volta apenas modelos de domínio.

#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard};

use anyhow::{anyhow, Result};

use crate::libs::models::{ConnectionProfile, KeychainEntry, VaultStatus};
use crate::libs::vault::VaultManager;

pub struct Backend {
    vault: Mutex<VaultManager>,
}

impl Backend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            vault: Mutex::new(VaultManager::new()?),
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

    pub fn connection_save(&self, profile: ConnectionProfile) -> Result<ConnectionProfile> {
        self.vault()?.connection_save(profile)
    }

    pub fn connection_delete(&self, id: &str) -> Result<()> {
        self.vault()?.connection_delete(id)
    }

    pub fn connection(&self, id: &str) -> Result<ConnectionProfile> {
        self.vault()?.profile_by_id(id)
    }

    pub fn keychain(&self) -> Result<Vec<KeychainEntry>> {
        self.vault()?.keychain_list()
    }

    fn vault(&self) -> Result<MutexGuard<'_, VaultManager>> {
        self.vault
            .lock()
            .map_err(|_| anyhow!("Estado do cofre ficou inconsistente"))
    }
}
