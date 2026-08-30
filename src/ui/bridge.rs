//! Ponte entre os callbacks declarados em `ui/main.slint` e a fachada `backend`.
//!
//! A UI só emite intenções; toda regra de negócio vive nos módulos de domínio.
//! Cada callback é registrado uma vez em `run` e usa um handle fraco da janela
//! para evitar o ciclo de referência que vazaria a `AppWindow`.

use std::sync::Arc;

use anyhow::Result;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::files_flow;
use super::keychain_flow;
use super::known_hosts_flow;
use super::mappers::{empty_draft, from_draft, to_draft, to_row};
use super::notes_flow;
use super::session_flow;
use super::settings_flow;
use super::terminal_view;
use super::{AppWindow, ConnectionDraft, VaultState};
use crate::backend::Backend;
use crate::libs::models::VaultStatus;

const MIN_PASSWORD_LEN: usize = 6;

pub fn run() -> Result<()> {
    let backend = Arc::new(Backend::new()?);
    let window = AppWindow::new()?;

    apply_vault_status(&window, backend.status()?);
    apply_environment(&window, &backend);
    bind_vault(&window, Arc::clone(&backend));
    bind_connections(&window, Arc::clone(&backend));
    bind_connection_form(&window, Arc::clone(&backend));
    session_flow::bind(&window, Arc::clone(&backend));
    // O temporizador de drenagem vive enquanto a janela viver.
    keychain_flow::bind(&window, Arc::clone(&backend));
    known_hosts_flow::bind(&window, Arc::clone(&backend));
    settings_flow::bind(&window, Arc::clone(&backend));
    notes_flow::bind(&window, Arc::clone(&backend));
    files_flow::bind(&window, Arc::clone(&backend));
    let _terminal_poll = terminal_view::bind(&window, backend);

    window.run()?;
    Ok(())
}

fn bind_vault(window: &AppWindow, backend: Arc<Backend>) {
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
                keychain_flow::refresh(&window, &backend);
                known_hosts_flow::refresh(&window, &backend);
                settings_flow::refresh(&window, &backend);
                notes_flow::refresh(&window, &backend);
            }
            Err(error) => window.set_vault_error(report(&error)),
        }
    });
}

fn bind_connections(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_connection_delete_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.connection_delete(&id) {
            Ok(()) => refresh_connections(&window, &backend),
            Err(error) => window.set_form_error(report(&error)),
        }
    });
}

fn bind_connection_form(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_connection_create_requested(move || {
        if let Some(window) = handle.upgrade() {
            open_form(&window, empty_draft());
        }
    });

    let handle = window.as_weak();
    let editing = Arc::clone(&backend);
    window.on_connection_edit_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match editing.connection(&id) {
            Ok(profile) => open_form(&window, to_draft(&profile)),
            Err(error) => window.set_form_error(report(&error)),
        }
    });

    let handle = window.as_weak();
    window.on_connection_form_dismissed(move || {
        if let Some(window) = handle.upgrade() {
            close_form(&window);
        }
    });

    let handle = window.as_weak();
    window.on_connection_saved(move |draft| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.connection_save(from_draft(&draft)) {
            Ok(_) => {
                close_form(&window);
                refresh_connections(&window, &backend);
            }
            Err(error) => window.set_form_error(report(&error)),
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

fn open_form(window: &AppWindow, draft: ConnectionDraft) {
    window.set_form_draft(draft);
    window.set_form_error(SharedString::new());
    window.set_form_open(true);
}

fn close_form(window: &AppWindow) {
    window.set_form_open(false);
    window.set_form_draft(empty_draft());
    window.set_form_error(SharedString::new());
}

pub fn apply_vault_status(window: &AppWindow, status: VaultStatus) {
    window.set_vault(VaultState {
        initialized: status.initialized,
        locked: status.locked,
        recoverable: status.recoverable,
    });
}

fn refresh_connections(window: &AppWindow, backend: &Backend) {
    match backend.connections() {
        Ok(profiles) => {
            let rows = profiles.iter().map(to_row).collect::<Vec<_>>();
            window.set_connections(ModelRc::new(VecModel::from(rows)));
        }
        Err(error) => window.set_form_error(report(&error)),
    }
}

/// Dados que não mudam durante a execução: versão do binário e onde o cofre
/// guarda seus arquivos.
fn apply_environment(window: &AppWindow, backend: &Backend) {
    window.set_version(env!("CARGO_PKG_VERSION").into());
    if let Ok(path) = backend.storage_path() {
        window.set_storage_path(path.into());
    }
}

/// Erros do domínio já vêm com contexto e sem segredos; a UI só os formata.
fn report(error: &anyhow::Error) -> SharedString {
    format!("{error}").into()
}
