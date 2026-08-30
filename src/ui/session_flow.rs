//! Fluxo de abertura de sessão SSH, incluindo o aceite de host desconhecido.
//!
//! A conexão roda fora da thread da interface; o desfecho volta pelo event loop
//! do Slint. Um host novo interrompe o fluxo e só prossegue depois que o usuário
//! confirma a impressão digital apresentada pelo servidor.

use std::sync::Arc;

use slint::ComponentHandle;

use super::{AppWindow, HostChallenge, Section, SessionRow};
use crate::backend::Backend;
use crate::libs::models::{SshConnectResult, SshSessionInfo};

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    let connecting = Arc::clone(&backend);
    window.on_connection_connect_requested(move |id| {
        start(&handle, &connecting, &id, false);
    });

    let handle = window.as_weak();
    let accepting = Arc::clone(&backend);
    window.on_challenge_accepted(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let pending = window.get_challenge();
        clear_challenge(&window);
        start(&handle, &accepting, pending.id.as_str(), true);
    });

    let handle = window.as_weak();
    window.on_challenge_rejected(move || {
        if let Some(window) = handle.upgrade() {
            clear_challenge(&window);
            window.set_status_message("Conexão cancelada: host não confirmado.".into());
        }
    });
}

fn start(handle: &slint::Weak<AppWindow>, backend: &Arc<Backend>, id: &str, accept_unknown: bool) {
    let Some(window) = handle.upgrade() else {
        return;
    };
    window.set_status_message("Conectando…".into());

    let profile_id = id.to_owned();
    let deliver = handle.clone();
    let after = Arc::clone(backend);

    let request = backend.connect(id, accept_unknown, move |outcome| {
        let _ = deliver.upgrade_in_event_loop(move |window| {
            match outcome {
                Ok(result) => apply(&window, &after, &profile_id, result),
                Err(error) => window.set_status_message(format!("{error}").into()),
            };
        });
    });

    if let Err(error) = request {
        window.set_status_message(format!("{error}").into());
    }
}

fn apply(window: &AppWindow, backend: &Backend, profile_id: &str, result: SshConnectResult) {
    match result {
        SshConnectResult::Connected { session } => {
            // O aceite de um host novo só vira parte do cofre depois que a
            // sessão abre; até lá ele existe apenas no arquivo de trabalho.
            if let Err(error) = backend.capture_known_hosts() {
                window.set_status_message(format!("{error}").into());
                return;
            }
            push_session(window, &session);
            // Um host recem-aceito ja entrou no cofre; a lista precisa refletir.
            super::known_hosts_flow::refresh(window, backend);
            window.set_active_session(session.session_id.as_str().into());
            window.set_section(Section::Terminal);
            window.set_status_message("Sessão aberta.".into());
        }
        SshConnectResult::UnknownHostChallenge {
            host,
            port,
            key_type,
            fingerprint,
            known_hosts_path,
            ..
        } => {
            window.set_challenge(HostChallenge {
                active: true,
                id: profile_id.into(),
                host: host.as_str().into(),
                port: port.to_string().into(),
                key_type: key_type.as_str().into(),
                fingerprint: fingerprint.as_str().into(),
                known_hosts_path: known_hosts_path.as_str().into(),
            });
            window.set_status_message("Confirme a impressão digital do servidor.".into());
        }
        SshConnectResult::AuthRequired { message } | SshConnectResult::Error { message } => {
            window.set_status_message(message.message.as_str().into());
        }
    }
}

fn push_session(window: &AppWindow, session: &SshSessionInfo) {
    use slint::{Model, ModelRc, VecModel};

    let mut rows: Vec<SessionRow> = window.get_sessions().iter().collect();
    rows.push(SessionRow {
        id: session.session_id.as_str().into(),
        profile_id: session.profile_id.as_str().into(),
        kind: session.session_kind.as_str().into(),
        label: session.profile_id.as_str().into(),
        connected_at: session.connected_at.to_string().into(),
    });
    window.set_sessions(ModelRc::new(VecModel::from(rows)));
}

fn clear_challenge(window: &AppWindow) {
    window.set_challenge(HostChallenge::default());
}
