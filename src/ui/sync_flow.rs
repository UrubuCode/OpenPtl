//! Sincronização do cofre com o Google Drive.
//!
//! Login, envio e recebimento rodam no runtime; o andamento chega pelo relator
//! compartilhado, lido num temporizador. Nenhuma etapa longa toca a thread de
//! desenho.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use super::AppWindow;
use crate::backend::Backend;

/// Cadência de leitura do andamento. O login espera ação do usuário no
/// navegador, então uma atualização por segundo basta.
const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

pub fn bind(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let handle = window.as_weak();
    let logging_in = Arc::clone(&backend);
    window.on_sync_login_requested(move || {
        if let Some(window) = handle.upgrade() {
            window.set_sync_busy(true);
            logging_in.sync_login();
        }
    });

    let handle = window.as_weak();
    let pushing = Arc::clone(&backend);
    window.on_sync_push_requested(move || {
        if let Some(window) = handle.upgrade() {
            window.set_sync_busy(true);
            pushing.sync_push();
        }
    });

    let handle = window.as_weak();
    let pulling = Arc::clone(&backend);
    window.on_sync_pull_requested(move || {
        if let Some(window) = handle.upgrade() {
            window.set_sync_busy(true);
            pulling.sync_pull();
        }
    });

    let cancelling = Arc::clone(&backend);
    window.on_sync_cancel_requested(move || {
        cancelling.sync_cancel();
    });

    let handle = window.as_weak();
    let leaving = Arc::clone(&backend);
    window.on_sync_logout_requested(move || {
        leaving.sync_logout();
        if let Some(window) = handle.upgrade() {
            refresh(&window, &leaving);
        }
    });

    start_refresh(window, backend)
}

fn start_refresh(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let timer = Timer::default();
    let handle = window.as_weak();

    timer.start(TimerMode::Repeated, REFRESH_INTERVAL, move || {
        if let Some(window) = handle.upgrade() {
            refresh(&window, &backend);
        }
    });

    timer
}

pub fn refresh(window: &AppWindow, backend: &Backend) {
    let user = backend.sync_user();
    window.set_sync_connected(user.is_some());
    window.set_sync_account(
        user.map(|user| account_label(&user))
            .unwrap_or_default()
            .into(),
    );

    let reporter = backend.sync_reporter();
    window.set_sync_message(reporter.message().message.as_str().into());
    window.set_sync_progress(
        reporter
            .current_progress()
            .map(|progress| progress.percent() as i32)
            .unwrap_or(0),
    );

    // Uma etapa concluída solta os botões: o estado final chega sem progresso.
    if reporter.current_progress().is_none() {
        window.set_sync_busy(false);
    }
}

/// Identificação da conta: o nome quando o servidor manda um, senão o e-mail.
fn account_label(user: &crate::libs::models::SyncLoggedUser) -> String {
    let name = user.name.as_deref().unwrap_or_default().trim().to_owned();
    if !name.is_empty() {
        return name;
    }
    user.email.as_deref().unwrap_or_default().trim().to_owned()
}
