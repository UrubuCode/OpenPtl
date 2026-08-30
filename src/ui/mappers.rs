//! Conversões entre modelos de domínio e as estruturas expostas ao Slint.
//!
//! Isolar a tradução aqui mantém a bridge focada em intenções do usuário e
//! garante que nenhum segredo do perfil vaze para campos apenas de exibição.

use slint::SharedString;

use super::{ConnectionDraft, ConnectionRow};
use crate::libs::models::{ConnectionProfile, ConnectionProtocol};

const DEFAULT_SSH_PORT: u16 = 22;

pub fn to_row(profile: &ConnectionProfile) -> ConnectionRow {
    ConnectionRow {
        id: profile.id.as_str().into(),
        name: profile.name.as_str().into(),
        host: profile.host.as_str().into(),
        port: profile.port.to_string().into(),
        username: profile.username.as_str().into(),
        protocols: protocol_summary(profile).into(),
        remote_path: profile.remote_path.clone().unwrap_or_default().into(),
        has_ssh: profile.supports(ConnectionProtocol::Ssh),
        has_sftp: profile.supports(ConnectionProtocol::Sftp),
    }
}

/// O rascunho carrega senha e chave porque o formulário precisa editá-las; a
/// linha da lista nunca recebe esses campos.
pub fn to_draft(profile: &ConnectionProfile) -> ConnectionDraft {
    ConnectionDraft {
        id: profile.id.as_str().into(),
        name: profile.name.as_str().into(),
        host: profile.host.as_str().into(),
        port: profile.port.to_string().into(),
        username: profile.username.as_str().into(),
        password: profile.password.clone().unwrap_or_default().into(),
        private_key: profile.private_key.clone().unwrap_or_default().into(),
        keychain_id: profile.keychain_id.clone().unwrap_or_default().into(),
        remote_path: profile.remote_path.clone().unwrap_or_default().into(),
        use_ssh: profile.supports(ConnectionProtocol::Ssh),
        use_sftp: profile.supports(ConnectionProtocol::Sftp),
    }
}

pub fn empty_draft() -> ConnectionDraft {
    ConnectionDraft {
        port: SharedString::from(DEFAULT_SSH_PORT.to_string()),
        use_ssh: true,
        use_sftp: true,
        ..Default::default()
    }
}

pub fn from_draft(draft: &ConnectionDraft) -> ConnectionProfile {
    let mut protocols = Vec::new();
    if draft.use_ssh {
        protocols.push(ConnectionProtocol::Ssh);
    }
    if draft.use_sftp {
        protocols.push(ConnectionProtocol::Sftp);
    }

    ConnectionProfile {
        id: draft.id.to_string(),
        name: draft.name.trim().to_string(),
        host: draft.host.trim().to_string(),
        port: draft.port.trim().parse().unwrap_or(DEFAULT_SSH_PORT),
        username: draft.username.trim().to_string(),
        password: optional(&draft.password),
        private_key: optional(&draft.private_key),
        keychain_id: optional(&draft.keychain_id),
        remote_path: optional(&draft.remote_path),
        protocols,
        kind: None,
    }
}

fn protocol_summary(profile: &ConnectionProfile) -> String {
    let mut labels = Vec::new();
    if profile.supports(ConnectionProtocol::Ssh) {
        labels.push("SSH");
    }
    if profile.supports(ConnectionProtocol::Sftp) {
        labels.push("SFTP");
    }
    labels.join(" · ")
}

/// Campo vazio no formulário significa "sem valor", não string vazia gravada.
fn optional(value: &SharedString) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
