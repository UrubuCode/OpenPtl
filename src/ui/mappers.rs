//! Conversões entre modelos de domínio e as estruturas expostas ao Slint.
//!
//! Isolar a tradução aqui mantém a bridge focada em intenções do usuário e
//! garante que nenhum segredo do perfil vaze para campos apenas de exibição.

use slint::SharedString;

use super::{ConnectionDraft, ConnectionRow};
use crate::libs::deeplink::DeepLink;
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

/// Rascunho pré-preenchido a partir de um endereço externo.
///
/// O link só sugere: nada é gravado no cofre e nenhuma conexão é aberta até o
/// usuário confirmar. Um endereço vindo de fora não deve conseguir fazer o
/// aplicativo conectar sozinho a um servidor arbitrário.
pub fn draft_from_link(link: &DeepLink) -> ConnectionDraft {
    ConnectionDraft {
        name: link.host.as_str().into(),
        host: link.host.as_str().into(),
        port: link.port.to_string().into(),
        username: link.username.clone().unwrap_or_default().into(),
        use_ssh: link.protocol == ConnectionProtocol::Ssh,
        use_sftp: link.protocol == ConnectionProtocol::Sftp,
        ..empty_draft()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(protocol: ConnectionProtocol, username: Option<&str>) -> DeepLink {
        DeepLink {
            protocol,
            host: "host.local".to_owned(),
            port: 2200,
            username: username.map(|value| value.to_owned()),
        }
    }

    #[test]
    fn a_link_fills_the_form_without_touching_the_vault() {
        let draft = draft_from_link(&link(ConnectionProtocol::Ssh, Some("deploy")));

        assert_eq!(draft.host, "host.local");
        assert_eq!(draft.port, "2200");
        assert_eq!(draft.username, "deploy");
        assert!(
            draft.id.is_empty(),
            "o rascunho nao aponta para um perfil salvo"
        );
        assert!(draft.password.is_empty(), "um link nunca traz segredo");
    }

    #[test]
    fn the_protocol_of_the_link_is_the_one_selected() {
        let sftp = draft_from_link(&link(ConnectionProtocol::Sftp, None));
        assert!(sftp.use_sftp);
        assert!(!sftp.use_ssh);
    }
}
