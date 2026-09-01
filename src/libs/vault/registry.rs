//! Índice dos cofres locais.
//!
//! Cada cofre é um diretório completo em `OpenPtl/vaults/<id>`, com o próprio
//! `openptl.bin`, log de mutações e pasta remota. Separar por diretório — e
//! não por prefixo de arquivo — é o que permitirá acrescentar cofres de origem
//! remota depois, já que um cofre inteiro passa a ser uma unidade movível.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use directories::ProjectDirs;

use super::crypto::{is_bin_file_name, read_bin_file, write_bin_file};
use crate::constants::{
    CURRENT_PAYLOAD_VERSION, CURRENT_STORAGE_VERSION, DEFAULT_VAULT_LABEL, LEGACY_PROFILE_DIR_NAME,
    LEGACY_VAULT_FILE_NAME, OPENPTL_FILE_NAME, STORAGE_DIR_NAME, VAULTS_DIR_NAME,
    VAULTS_REGISTRY_FILE_NAME, VAULT_LABEL_MAX_LEN,
};
use crate::libs::models::{VaultEntry, VaultsBinPayload};

pub struct VaultRegistry {
    storage_root: PathBuf,
    vaults_root: PathBuf,
    registry_path: PathBuf,
    payload: VaultsBinPayload,
}

impl VaultRegistry {
    /// Resolve o diretório de dados pelo padrão do sistema.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "urubucode", "openptl")
            .ok_or_else(|| anyhow!("Nao foi possivel resolver diretorio de dados do aplicativo"))?;
        Self::new_in(dirs.data_dir().to_path_buf())
    }

    pub fn new_in(data_dir: PathBuf) -> Result<Self> {
        let storage_root = data_dir.join(STORAGE_DIR_NAME);
        let vaults_root = storage_root.join(VAULTS_DIR_NAME);
        fs::create_dir_all(&vaults_root)
            .with_context(|| format!("Falha ao criar diretorio {}", vaults_root.display()))?;

        let registry_path = storage_root.join(VAULTS_REGISTRY_FILE_NAME);
        let payload = if registry_path.exists() {
            read_bin_file(&registry_path).unwrap_or_default()
        } else {
            VaultsBinPayload::default()
        };

        let mut registry = Self {
            storage_root,
            vaults_root,
            registry_path,
            payload,
        };

        registry.purge_legacy_artifacts();
        registry.adopt_single_vault_layout()?;
        registry.forget_missing()?;
        registry.drop_outdated_vaults()?;
        registry.ensure_selection()?;
        Ok(registry)
    }

    /// Apaga restos de instalações anteriores que nenhuma versão atual lê.
    ///
    /// São o `vault.enc.json` do tempo do Tauri e a pasta `default` do layout
    /// que precedeu os cofres por diretório. Falhar aqui não interrompe a
    /// abertura: é limpeza, não pré-requisito.
    fn purge_legacy_artifacts(&self) {
        if let Some(data_dir) = self.storage_root.parent() {
            let _ = fs::remove_file(data_dir.join(LEGACY_VAULT_FILE_NAME));
        }

        let legacy_default = self.storage_root.join(LEGACY_PROFILE_DIR_NAME);
        if legacy_default.is_dir() {
            let _ = fs::remove_dir_all(&legacy_default);
        }
    }

    /// Remove cofres gravados num formato que esta versão não abre mais.
    ///
    /// Só apaga o que consegue identificar como anterior: um `openptl.bin`
    /// legível cuja versão é menor que a atual. Arquivo ilegível ou de versão
    /// mais nova fica onde está — pode ser um cofre gravado por uma versão
    /// posterior do aplicativo, e apagá-lo destruiria dados válidos.
    fn drop_outdated_vaults(&mut self) -> Result<()> {
        let outdated: Vec<String> = self
            .payload
            .vaults
            .iter()
            .filter(|entry| is_outdated_vault(&self.vaults_root.join(&entry.id)))
            .map(|entry| entry.id.clone())
            .collect();

        if outdated.is_empty() {
            return Ok(());
        }

        for id in &outdated {
            let _ = fs::remove_dir_all(self.vaults_root.join(id));
        }
        self.payload
            .vaults
            .retain(|entry| !outdated.contains(&entry.id));
        self.persist()
    }

    /// Move uma instalação de cofre único para dentro de `vaults/<id>`.
    ///
    /// Sem isto o usuário perderia o cofre ao atualizar: os arquivos soltos na
    /// raiz deixariam de ser encontrados e pareceriam um cofre inexistente.
    fn adopt_single_vault_layout(&mut self) -> Result<()> {
        if !self.storage_root.join(OPENPTL_FILE_NAME).exists() {
            return Ok(());
        }

        // Cofre único gravado no formato anterior: adotá-lo só adiaria o erro
        // para a tela de senha, que não teria como abri-lo.
        if is_outdated_vault(&self.storage_root) {
            return self.discard_single_vault_layout();
        }

        let entry = self.new_entry(DEFAULT_VAULT_LABEL);
        let target = self.vaults_root.join(&entry.id);
        fs::create_dir_all(&target)
            .with_context(|| format!("Falha ao criar diretorio {}", target.display()))?;

        for item in fs::read_dir(&self.storage_root)
            .with_context(|| format!("Falha ao listar {}", self.storage_root.display()))?
        {
            let path = item?.path();
            if path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name() else {
                continue;
            };
            // Só os arquivos do cofre. O `known_hosts` em claro é regenerado no
            // desbloqueio, e restos do instalador antigo não viajam junto.
            if name == VAULTS_REGISTRY_FILE_NAME || !is_bin_file_name(&name.to_string_lossy()) {
                continue;
            }
            fs::rename(&path, target.join(name))
                .with_context(|| format!("Falha ao mover {}", path.display()))?;
        }

        let mut adopted = entry;
        adopted.initialized = true;
        self.payload.selected = Some(adopted.id.clone());
        self.payload.vaults.push(adopted);
        self.persist()
    }

    /// Apaga os arquivos soltos na raiz do cofre único antigo.
    fn discard_single_vault_layout(&self) -> Result<()> {
        for item in fs::read_dir(&self.storage_root)
            .with_context(|| format!("Falha ao listar {}", self.storage_root.display()))?
        {
            let path = item?.path();
            if path.is_dir() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
            else {
                continue;
            };
            if name == VAULTS_REGISTRY_FILE_NAME {
                continue;
            }
            let _ = fs::remove_file(&path);
        }
        Ok(())
    }

    /// Descarta do índice cofres cujo diretório sumiu — apagados à mão, por
    /// exemplo. Mantê-los faria a interface oferecer um cofre que não abre.
    fn forget_missing(&mut self) -> Result<()> {
        let vaults_root = self.vaults_root.clone();
        let before = self.payload.vaults.len();
        self.payload
            .vaults
            .retain(|entry| vaults_root.join(&entry.id).is_dir());

        if self.payload.vaults.len() == before {
            return Ok(());
        }
        self.persist()
    }

    fn ensure_selection(&mut self) -> Result<()> {
        let valid = self
            .payload
            .selected
            .as_ref()
            .map(|id| self.payload.vaults.iter().any(|entry| &entry.id == id))
            .unwrap_or(false);
        if valid {
            return Ok(());
        }

        self.payload.selected = self.payload.vaults.first().map(|entry| entry.id.clone());
        self.persist()
    }

    fn new_entry(&self, label: &str) -> VaultEntry {
        VaultEntry {
            // v7 para que a listagem saia em ordem de criação sem guardar
            // índice à parte.
            id: uuid::Uuid::now_v7().to_string(),
            label: normalize_label(label),
            created_at: Utc::now().timestamp(),
            initialized: false,
        }
    }

    pub fn list(&self) -> Vec<VaultEntry> {
        self.payload.vaults.clone()
    }

    pub fn selected_id(&self) -> Option<String> {
        self.payload.selected.clone()
    }

    pub fn selected(&self) -> Option<VaultEntry> {
        let id = self.payload.selected.as_ref()?;
        self.payload
            .vaults
            .iter()
            .find(|entry| &entry.id == id)
            .cloned()
    }

    /// Diretório do cofre selecionado, criado se ainda não existir.
    pub fn selected_path(&self) -> Result<PathBuf> {
        let id = self
            .selected_id()
            .ok_or_else(|| anyhow!("Nenhum cofre selecionado"))?;
        self.path_of(&id)
    }

    pub fn path_of(&self, id: &str) -> Result<PathBuf> {
        let entry = self
            .payload
            .vaults
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| anyhow!("Cofre {} nao encontrado", id))?;
        let path = self.vaults_root.join(&entry.id);
        fs::create_dir_all(&path)
            .with_context(|| format!("Falha ao criar diretorio {}", path.display()))?;
        Ok(path)
    }

    pub fn create(&mut self, label: &str) -> Result<VaultEntry> {
        let label = normalize_label(label);
        if label.is_empty() {
            return Err(anyhow!("Informe um nome para o cofre"));
        }
        if self
            .payload
            .vaults
            .iter()
            .any(|entry| entry.label.eq_ignore_ascii_case(&label))
        {
            return Err(anyhow!("Ja existe um cofre com esse nome"));
        }

        let entry = self.new_entry(&label);
        fs::create_dir_all(self.vaults_root.join(&entry.id))
            .with_context(|| format!("Falha ao criar diretorio do cofre {}", entry.id))?;

        self.payload.selected = Some(entry.id.clone());
        self.payload.vaults.push(entry.clone());
        self.persist()?;
        Ok(entry)
    }

    pub fn select(&mut self, id: &str) -> Result<VaultEntry> {
        let entry = self
            .payload
            .vaults
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Cofre {} nao encontrado", id))?;

        self.payload.selected = Some(entry.id.clone());
        self.persist()?;
        Ok(entry)
    }

    pub fn rename(&mut self, id: &str, label: &str) -> Result<VaultEntry> {
        let label = normalize_label(label);
        if label.is_empty() {
            return Err(anyhow!("Informe um nome para o cofre"));
        }
        if self
            .payload
            .vaults
            .iter()
            .any(|entry| entry.id != id && entry.label.eq_ignore_ascii_case(&label))
        {
            return Err(anyhow!("Ja existe um cofre com esse nome"));
        }

        let entry = self
            .payload
            .vaults
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| anyhow!("Cofre {} nao encontrado", id))?;
        entry.label = label;
        let updated = entry.clone();
        self.persist()?;
        Ok(updated)
    }

    /// Marca que o cofre passou a ter senha mestre.
    pub fn mark_initialized(&mut self, id: &str, initialized: bool) -> Result<()> {
        let Some(entry) = self.payload.vaults.iter_mut().find(|item| item.id == id) else {
            return Ok(());
        };
        if entry.initialized == initialized {
            return Ok(());
        }
        entry.initialized = initialized;
        self.persist()
    }

    /// Apaga o cofre e todo o conteúdo dele no disco.
    ///
    /// O conteúdo remoto não é tocado: apagar a pasta do Drive a partir de um
    /// aparelho destruiria o cofre para todos os outros, e essa é uma decisão
    /// que precisa de um gesto próprio.
    pub fn remove(&mut self, id: &str) -> Result<()> {
        if !self.payload.vaults.iter().any(|entry| entry.id == id) {
            return Err(anyhow!("Cofre {} nao encontrado", id));
        }

        let path = self.vaults_root.join(id);
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("Falha ao remover {}", path.display()))?;
        }

        self.payload.vaults.retain(|entry| entry.id != id);
        self.ensure_selection()?;
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        let payload = VaultsBinPayload {
            version: CURRENT_PAYLOAD_VERSION,
            selected: self.payload.selected.clone(),
            vaults: self.payload.vaults.clone(),
        };
        write_bin_file(&self.registry_path, &payload)
    }

    pub fn storage_root(&self) -> &Path {
        &self.storage_root
    }
}

/// Um diretório guarda um cofre de formato anterior?
///
/// Um cofre ainda sem `openptl.bin` é novo, não velho. A versão só conta como
/// antiga quando pode ser lida e é menor que a atual.
fn is_outdated_vault(dir: &Path) -> bool {
    let path = dir.join(OPENPTL_FILE_NAME);
    if !path.exists() {
        return false;
    }

    read_bin_file::<super::OpenPtlBin>(&path)
        .map(|file| file.version < CURRENT_STORAGE_VERSION)
        .unwrap_or(false)
}

fn normalize_label(input: &str) -> String {
    input.trim().chars().take(VAULT_LABEL_MAX_LEN).collect()
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
