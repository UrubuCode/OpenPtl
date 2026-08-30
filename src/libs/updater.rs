//! Verificação e download de atualizações.
//!
//! O plugin do Tauri fazia isso com assinaturas minisign. A chave pública é a
//! mesma de antes, então todos os artefatos já publicados continuam válidos e o
//! processo de assinatura na esteira não muda. O que este módulo não faz de
//! jeito nenhum é instalar um binário sem conferir a assinatura: sem ela, uma
//! atualização vira um caminho de execução remota de código.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;

use crate::constants::{
    RELEASE_MANIFEST_CANARY_URL, RELEASE_MANIFEST_STABLE_URL, RELEASE_PUBLIC_KEY,
    RELEASE_USER_AGENT,
};

/// Canal de atualização escolhido pelo usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    #[default]
    Stable,
    Canary,
}

impl Channel {
    pub fn manifest_url(self) -> &'static str {
        match self {
            Channel::Stable => RELEASE_MANIFEST_STABLE_URL,
            Channel::Canary => RELEASE_MANIFEST_CANARY_URL,
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.trim().to_ascii_lowercase().as_str() {
            "canary" => Channel::Canary,
            _ => Channel::Stable,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Canary => "canary",
        }
    }
}

/// Manifesto publicado junto da release, no formato que a esteira já gera.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    pub platforms: std::collections::HashMap<String, PlatformRelease>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlatformRelease {
    pub signature: String,
    pub url: String,
}

/// Resultado de uma consulta de atualização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
}

pub struct Updater {
    client: reqwest::Client,
    current: Version,
}

impl Updater {
    pub fn new(current_version: &str) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent(RELEASE_USER_AGENT)
                .build()
                .context("Falha ao preparar cliente de atualizacao")?,
            current: Version::parse(current_version)
                .with_context(|| format!("Versao atual invalida: {current_version}"))?,
        })
    }

    pub async fn check(&self, channel: Channel) -> Result<Availability> {
        let manifest = self.manifest(channel).await?;
        let latest = Version::parse(manifest.version.trim_start_matches('v'))
            .with_context(|| format!("Versao publicada invalida: {}", manifest.version))?;

        Ok(Availability {
            available: latest > self.current,
            current: self.current.to_string(),
            latest: latest.to_string(),
            notes: manifest.notes,
        })
    }

    /// Baixa o instalador da plataforma atual e só devolve o caminho depois de
    /// a assinatura conferir. Um arquivo com assinatura inválida é descartado.
    pub async fn download(&self, channel: Channel, destination: &Path) -> Result<PathBuf> {
        let manifest = self.manifest(channel).await?;
        let release = manifest
            .platforms
            .get(platform_key())
            .ok_or_else(|| anyhow!("Sem artefato publicado para {}", platform_key()))?;

        let bytes = self
            .client
            .get(&release.url)
            .send()
            .await
            .with_context(|| format!("Falha ao baixar {}", release.url))?
            .error_for_status()
            .with_context(|| format!("Resposta de erro ao baixar {}", release.url))?
            .bytes()
            .await
            .context("Falha ao ler o corpo da atualizacao")?;

        verify(&bytes, &release.signature)?;

        let file_name = file_name_of(&release.url);
        let path = destination.join(file_name);
        std::fs::create_dir_all(destination).ok();
        std::fs::write(&path, &bytes)
            .with_context(|| format!("Falha ao gravar {}", path.display()))?;

        Ok(path)
    }

    async fn manifest(&self, channel: Channel) -> Result<Manifest> {
        self.client
            .get(channel.manifest_url())
            .send()
            .await
            .context("Falha ao consultar o manifesto de versoes")?
            .error_for_status()
            .context("Resposta de erro ao consultar o manifesto")?
            .json()
            .await
            .context("Manifesto de versoes invalido")
    }
}

/// Confere a assinatura minisign do artefato contra a chave pública embutida.
pub fn verify(bytes: &[u8], signature: &str) -> Result<()> {
    // `decode` espera o arquivo inteiro: comentário na primeira linha, chave na
    // segunda. Passar só a chave faz o comentário ser lido como chave.
    let key = PublicKey::decode(RELEASE_PUBLIC_KEY)
        .map_err(|error| anyhow!("Chave publica de release invalida: {error}"))?;

    let signature = Signature::decode(signature)
        .map_err(|error| anyhow!("Assinatura da atualizacao invalida: {error}"))?;

    key.verify(bytes, &signature, false)
        .map_err(|_| anyhow!("A atualizacao nao confere com a assinatura publicada"))
}

/// Chave da plataforma no manifesto, no formato que a esteira já usa.
pub fn platform_key() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "darwin-aarch64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    }
}

fn file_name_of(url: &str) -> String {
    url.rsplit('/')
        .find(|part| !part.is_empty())
        .and_then(|name| name.split('?').next())
        .unwrap_or("openptl-update")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_labels_round_trip() {
        assert_eq!(Channel::from_label("canary"), Channel::Canary);
        assert_eq!(Channel::from_label("CANARY"), Channel::Canary);
        assert_eq!(Channel::from_label("stable"), Channel::Stable);
        assert_eq!(Channel::from_label("qualquer coisa"), Channel::Stable);
        assert_eq!(Channel::Canary.label(), "canary");
    }

    #[test]
    fn channels_point_at_different_manifests() {
        assert_ne!(
            Channel::Stable.manifest_url(),
            Channel::Canary.manifest_url()
        );
    }

    #[test]
    fn a_tampered_artifact_is_refused() {
        // Assinatura sintaticamente válida, conteúdo que não corresponde.
        let signature = concat!(
            "untrusted comment: signature from minisign secret key\n",
            "RUSBfjuAckIwbomZ1kfmZ0Jf1RTLNb0ry0jJ2P8SqfXCFN8UyBEHkkZbrKNMTs1a",
            "Cq6kbYzHo2xUlx1fMv0ByBHqfDgcnwv5xgg=\n",
            "trusted comment: timestamp:0\n",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000000000000000000000000000000000000000000000000000000\n"
        );

        let outcome = verify(b"binario adulterado", signature);
        assert!(outcome.is_err(), "assinatura divergente deve ser recusada");
    }

    #[test]
    fn a_malformed_signature_is_refused() {
        assert!(verify(b"qualquer coisa", "nao e uma assinatura").is_err());
    }

    #[test]
    fn the_embedded_public_key_is_readable() {
        assert!(
            PublicKey::decode(RELEASE_PUBLIC_KEY).is_ok(),
            "a chave publica embutida deve ser decodificavel"
        );
    }

    /// A chave precisa continuar sendo a mesma que assinou as releases já
    /// publicadas; trocá-la sem querer invalidaria todas elas de uma vez.
    #[test]
    fn the_public_key_matches_the_one_used_by_the_published_releases() {
        assert!(
            RELEASE_PUBLIC_KEY.contains("RWSBfjuAckIwbu3kj/A7fXPqRAm0U4Vdh6hB//vYmtuMTglvEJrhxqZx")
        );
    }

    #[test]
    fn the_artifact_name_comes_from_the_url() {
        assert_eq!(
            file_name_of("https://exemplo.com/releases/OpenPtl_1.2.3_x64.msi"),
            "OpenPtl_1.2.3_x64.msi"
        );
        assert_eq!(file_name_of("https://exemplo.com/a.msi?token=abc"), "a.msi");
    }

    #[test]
    fn the_platform_key_is_one_of_the_published_ones() {
        let known = [
            "windows-x86_64",
            "darwin-aarch64",
            "darwin-x86_64",
            "linux-x86_64",
        ];
        assert!(known.contains(&platform_key()));
    }
}

/// Canal escolhido, guardado num arquivo simples ao lado do cofre.
///
/// Deliberadamente fora de `profile.bin`: aquele arquivo é posicional, e um
/// campo novo invalidaria os vaults já existentes. O canal também não é
/// segredo, então não precisa da proteção do cofre.
pub fn read_channel(directory: &Path) -> Channel {
    std::fs::read_to_string(directory.join(CHANNEL_FILE_NAME))
        .map(|label| Channel::from_label(&label))
        .unwrap_or_default()
}

pub fn write_channel(directory: &Path, channel: Channel) -> Result<()> {
    std::fs::create_dir_all(directory).ok();
    let path = directory.join(CHANNEL_FILE_NAME);
    std::fs::write(&path, channel.label())
        .with_context(|| format!("Falha ao gravar {}", path.display()))
}

const CHANNEL_FILE_NAME: &str = "update-channel";

#[cfg(test)]
mod channel_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn the_channel_survives_a_round_trip() {
        let directory = tempdir().expect("temp dir");
        write_channel(directory.path(), Channel::Canary).expect("write");
        assert_eq!(read_channel(directory.path()), Channel::Canary);
    }

    #[test]
    fn a_missing_file_means_stable() {
        let directory = tempdir().expect("temp dir");
        assert_eq!(read_channel(directory.path()), Channel::Stable);
    }

    #[test]
    fn unknown_content_falls_back_to_stable() {
        let directory = tempdir().expect("temp dir");
        std::fs::write(directory.path().join(CHANNEL_FILE_NAME), "lixo").expect("write");
        assert_eq!(read_channel(directory.path()), Channel::Stable);
    }
}
