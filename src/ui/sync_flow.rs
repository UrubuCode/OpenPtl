//! Sincronização do cofre com o Google Drive.
//!
//! Login, envio e recebimento rodam no runtime; o andamento chega pelo relator
//! compartilhado, lido num temporizador. Nenhuma etapa longa toca a thread de
//! desenho.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use super::{AppWindow, ServerRow};
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

    let handle = window.as_weak();
    let choosing = Arc::clone(&backend);
    window.on_server_selected(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match choosing.select_auth_server(&id) {
            Ok(()) => refresh_servers(&window, &choosing),
            Err(error) => window.set_sync_message(format!("{error}").into()),
        }
    });

    refresh_servers(window, &backend);
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
    let label = user.map(|user| account_label(&user)).unwrap_or_default();
    window.set_sync_initials(initials_of(&label).as_str().into());
    window.set_sync_account(label.as_str().into());

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

/// Lista de servidores de autenticação e qual está escolhido.
pub fn refresh_servers(window: &AppWindow, backend: &Backend) {
    let Ok(servers) = backend.auth_servers() else {
        return;
    };
    let selected = backend
        .selected_auth_server()
        .map(|server| server.id)
        .unwrap_or_default();

    let rows: Vec<ServerRow> = servers
        .iter()
        .map(|server| ServerRow {
            id: server.id.as_str().into(),
            label: server.label.as_str().into(),
            address: server.address.as_str().into(),
        })
        .collect();

    window.set_servers(ModelRc::new(VecModel::from(rows)));
    window.set_selected_server(selected.as_str().into());
}

/// Iniciais mostradas no avatar. O Slint não indexa string, então a montagem
/// acontece aqui.
fn initials_of(label: &str) -> String {
    let mut parts = label.split_whitespace();
    let first = parts.next().and_then(|part| part.chars().next());
    let second = parts.next().and_then(|part| part.chars().next());

    match (first, second) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::initials_of;

    #[test]
    fn two_names_give_two_letters() {
        assert_eq!(initials_of("Daniel Souza"), "DS");
    }

    #[test]
    fn one_name_gives_one_letter() {
        assert_eq!(initials_of("daniel"), "D");
    }

    #[test]
    fn an_email_uses_its_first_letter() {
        assert_eq!(initials_of("urubucode@gmail.com"), "U");
    }

    #[test]
    fn no_account_shows_a_question_mark() {
        assert_eq!(initials_of(""), "?");
        assert_eq!(initials_of("   "), "?");
    }
}
