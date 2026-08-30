//! Ponte entre os callbacks declarados em `ui/main.slint` e a fachada `backend`.
//!
//! A UI só emite intenções; toda regra de negócio vive nos módulos de domínio.
//! Cada callback é registrado uma vez em `run` e usa um handle fraco da janela
//! para evitar o ciclo de referência que vazaria a `AppWindow`.

use std::rc::Rc;

use anyhow::Result;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{AppWindow, ConnectionRow, VaultState};
use crate::backend::Backend;
use crate::libs::models::{ConnectionProfile, ConnectionProtocol, VaultStatus};

const MIN_PASSWORD_LEN: usize = 6;

pub fn run() -> Result<()> {
    let backend = Rc::new(Backend::new()?);
    let window = AppWindow::new()?;

    apply_vault_status(&window, backend.status()?);
    bind_vault(&window, Rc::clone(&backend));
    bind_connections(&window, backend);

    window.run()?;
    Ok(())
}

fn bind_vault(window: &AppWindow, backend: Rc<Backend>) {
    let handle = window.as_weak();
    window.on_vault_submitted(move |password, confirmation| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let initialized = window.get_vault().initialized;

        if let Some(problem) = validate_password(&password, &confirmation, initialized) {
            window.set_vault_error(problem.into());
            return;
        }

        window.set_vault_error(SharedString::new());
        window.set_vault_busy(true);

        let outcome = if initialized {
            backend.unlock(&password)
        } else {
            backend.initialize(&password)
        };

        window.set_vault_busy(false);

        match outcome {
            Ok(status) => {
                apply_vault_status(&window, status);
                refresh_connections(&window, &backend);
            }
            Err(error) => window.set_vault_error(format!("{error}").into()),
        }
    });
}

fn bind_connections(window: &AppWindow, backend: Rc<Backend>) {
    let handle = window.as_weak();
    window.on_connection_delete_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.connection_delete(&id) {
            Ok(()) => refresh_connections(&window, &backend),
            Err(error) => window.set_vault_error(format!("{error}").into()),
        }
    });
}

/// Uma senha vazia, curta demais ou divergente da confirmação nunca chega ao
/// vault: falhar aqui evita expor a diferença entre senha inválida e incorreta.
fn validate_password(password: &str, confirmation: &str, initialized: bool) -> Option<String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Some(format!(
            "A senha mestre precisa de ao menos {MIN_PASSWORD_LEN} caracteres."
        ));
    }
    if !initialized && password != confirmation {
        return Some("As senhas não coincidem.".to_owned());
    }
    None
}

fn apply_vault_status(window: &AppWindow, status: VaultStatus) {
    window.set_vault(VaultState {
        initialized: status.initialized,
        locked: status.locked,
        recoverable: status.recoverable,
    });
}

fn refresh_connections(window: &AppWindow, backend: &Backend) {
    match backend.connections() {
        Ok(profiles) => {
            let rows: Vec<ConnectionRow> = profiles.iter().map(to_row).collect();
            window.set_connections(ModelRc::new(VecModel::from(rows)));
        }
        Err(error) => window.set_vault_error(format!("{error}").into()),
    }
}

fn to_row(profile: &ConnectionProfile) -> ConnectionRow {
    ConnectionRow {
        id: profile.id.as_str().into(),
        name: profile.name.as_str().into(),
        host: profile.host.as_str().into(),
        port: profile.port.to_string().into(),
        username: profile.username.as_str().into(),
        protocols: SharedString::new(),
        remote_path: profile.remote_path.clone().unwrap_or_default().into(),
        has_ssh: profile.supports(ConnectionProtocol::Ssh),
        has_sftp: profile.supports(ConnectionProtocol::Sftp),
    }
}
