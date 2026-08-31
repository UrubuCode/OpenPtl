//! Ponte entre os callbacks declarados em `ui/main.slint` e a fachada `backend`.
//!
//! A UI só emite intenções; toda regra de negócio vive nos módulos de domínio.
//! Cada callback é registrado uma vez em `run` e usa um handle fraco da janela
//! para evitar o ciclo de referência que vazaria a `AppWindow`.

use std::sync::Arc;

use anyhow::Result;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::dev_unlock;
use super::editor_flow;
use super::files_flow;
use super::keychain_flow;
use super::known_hosts_flow;
use super::logs_flow;
use super::mappers::{draft_from_link, empty_draft, from_draft, to_draft, to_row};
use super::notes_flow;
use super::session_flow;
use super::settings_flow;
use super::sync_flow;
use super::terminal_view;
use super::transfers_flow;
use super::update_flow;
use super::window_flow;
use super::workspace_flow;
use super::{AppWindow, ConnectionDraft, ConnectionGridRow, ConnectionRow, VaultRow, VaultState};
use crate::backend::Backend;
use crate::libs::deeplink;
use crate::libs::models::{ConnectionProfile, ConnectionProtocol, VaultStatus};

const MIN_PASSWORD_LEN: usize = 6;

pub fn run() -> Result<()> {
    let backend = Arc::new(Backend::new()?);
    let window = AppWindow::new()?;

    apply_vault_status(&window, backend.status()?);
    refresh_vaults(&window, &backend);
    apply_environment(&window, &backend);
    window_flow::bind(&window);
    window_flow::center(&window);
    workspace_flow::bind(&window, Arc::clone(&backend));
    bind_vault(&window, Arc::clone(&backend));
    bind_connections(&window, Arc::clone(&backend));
    bind_connection_filters(&window, Arc::clone(&backend));
    bind_connection_picker(&window, Arc::clone(&backend));
    bind_connection_detail(&window, Arc::clone(&backend));
    bind_connection_form(&window, Arc::clone(&backend));
    session_flow::bind(&window, Arc::clone(&backend));
    // O temporizador de drenagem vive enquanto a janela viver.
    keychain_flow::bind(&window, Arc::clone(&backend));
    known_hosts_flow::bind(&window, Arc::clone(&backend));
    let _logs_refresh = logs_flow::bind(&window, Arc::clone(&backend));
    settings_flow::bind(&window, Arc::clone(&backend));
    notes_flow::bind(&window, Arc::clone(&backend));
    files_flow::bind(&window, Arc::clone(&backend));
    let _transfers_refresh = transfers_flow::bind(&window, Arc::clone(&backend));
    editor_flow::bind(&window, Arc::clone(&backend));
    let _sync_refresh = sync_flow::bind(&window, Arc::clone(&backend));
    update_flow::bind(&window, Arc::clone(&backend));
    let _terminal_poll = terminal_view::bind(&window, Arc::clone(&backend));

    try_dev_unlock(&window);

    window.run()?;
    Ok(())
}

/// Lista de cofres e qual está selecionado.
pub fn refresh_vaults(window: &AppWindow, backend: &Backend) {
    let Ok(entries) = backend.vaults() else {
        return;
    };
    let selected = backend
        .selected_vault()
        .ok()
        .flatten()
        .map(|entry| entry.id)
        .unwrap_or_default();

    let rows: Vec<VaultRow> = entries
        .iter()
        .map(|entry| VaultRow {
            id: entry.id.as_str().into(),
            label: entry.label.as_str().into(),
            initialized: entry.initialized,
        })
        .collect();

    window.set_vaults(ModelRc::new(VecModel::from(rows)));
    window.set_selected_vault(selected.as_str().into());
}

/// Aplica a troca de cofre: a tela volta para o formulário de senha, agora
/// apontando para o cofre novo. Nada do cofre anterior sobrevive porque a
/// fachada tranca antes de trocar.
fn adopt_vault(window: &AppWindow, backend: &Backend, outcome: Result<VaultStatus, anyhow::Error>) {
    match outcome {
        Ok(status) => {
            window.set_vault_error(SharedString::new());
            apply_vault_status(window, status);
            refresh_vaults(window, backend);
        }
        Err(error) => window.set_vault_error(report(&error)),
    }
}

fn bind_vault(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    let choosing = Arc::clone(&backend);
    window.on_vault_selected(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let outcome = choosing.vault_select(&id);
        adopt_vault(&window, &choosing, outcome);
    });

    let handle = window.as_weak();
    let creating = Arc::clone(&backend);
    window.on_vault_created(move |label| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let outcome = creating.vault_create(&label);
        adopt_vault(&window, &creating, outcome);
    });

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
                refresh_vaults(&window, &backend);
                refresh_connections(&window, &backend);
                refresh_picker(&window, &backend);
                keychain_flow::refresh(&window, &backend);
                known_hosts_flow::refresh(&window, &backend);
                settings_flow::refresh(&window, &backend);
                notes_flow::refresh(&window, &backend);
                apply_startup_link(&window);
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

/// Quantos cartões cabem por linha da grade.
const GRID_COLUMNS: usize = 3;

fn refresh_connections(window: &AppWindow, backend: &Backend) {
    let profiles = match backend.connections() {
        Ok(profiles) => profiles,
        Err(error) => {
            window.set_form_error(report(&error));
            return;
        }
    };

    // As contagens descrevem o cofre inteiro; a busca e o filtro só decidem o
    // que aparece na lista.
    window.set_connection_total(profiles.len() as i32);
    window.set_connection_ssh(count_of(&profiles, ConnectionProtocol::Ssh));
    window.set_connection_sftp(count_of(&profiles, ConnectionProtocol::Sftp));

    let query = window.get_connection_query().to_lowercase();
    let filter = window.get_connection_filter().to_string();

    let visible: Vec<ConnectionRow> = profiles
        .iter()
        .filter(|profile| matches_filter(profile, &filter))
        .filter(|profile| matches_query(profile, &query))
        .map(to_row)
        .collect();

    window.set_connection_empty(visible.is_empty());
    window.set_connection_grid(ModelRc::new(VecModel::from(grid_of(visible))));
}

/// Agrupa os cartões em linhas. O `for` do Slint percorre um modelo linear e
/// não sabe montar colunas, então a grade é montada aqui.
fn grid_of(rows: Vec<ConnectionRow>) -> Vec<ConnectionGridRow> {
    rows.chunks(GRID_COLUMNS)
        .map(|chunk| ConnectionGridRow {
            cells: ModelRc::new(VecModel::from(chunk.to_vec())),
        })
        .collect()
}

fn count_of(profiles: &[ConnectionProfile], protocol: ConnectionProtocol) -> i32 {
    profiles
        .iter()
        .filter(|profile| profile.supports(protocol.clone()))
        .count() as i32
}

/// Vazio significa todos os protocolos.
fn matches_filter(profile: &ConnectionProfile, filter: &str) -> bool {
    match filter {
        "ssh" => profile.supports(ConnectionProtocol::Ssh),
        "sftp" => profile.supports(ConnectionProtocol::Sftp),
        _ => true,
    }
}

/// A busca olha nome, host e usuário: é por um deles que alguém procura um
/// servidor.
fn matches_query(profile: &ConnectionProfile, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    profile.name.to_lowercase().contains(query)
        || profile.host.to_lowercase().contains(query)
        || profile.username.to_lowercase().contains(query)
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

/// Um endereço passado na linha de comando — inclusive o que o sistema entrega
/// ao abrir um link `openptl://` — abre o formulário já preenchido. Ele nunca
/// conecta sozinho: um link de terceiros não deve conseguir apontar o
/// aplicativo para um servidor arbitrário sem o usuário ver e confirmar.
fn apply_startup_link(window: &AppWindow) {
    let Some(link) = deeplink::from_arguments(std::env::args()) else {
        return;
    };

    window.set_form_draft(draft_from_link(&link));
    window.set_form_error(SharedString::new());
    window.set_form_open(true);
}

/// Abre o cofre sozinho quando há senha de desenvolvimento configurada. Em
/// release `dev_unlock::password` devolve sempre `None`, então isto não faz
/// nada — ver `ui/dev_unlock.rs`.
fn try_dev_unlock(window: &AppWindow) {
    let Some(password) = dev_unlock::password() else {
        return;
    };
    window.invoke_vault_submitted(password.as_str().into(), password.as_str().into());
}

/// Busca e filtro apenas reconstroem a lista; nada é gravado no cofre.
fn bind_connection_filters(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    let searching = Arc::clone(&backend);
    window.on_connection_query_changed(move |_| {
        if let Some(window) = handle.upgrade() {
            refresh_connections(&window, &searching);
        }
    });

    let handle = window.as_weak();
    window.on_connection_filter_changed(move |_| {
        if let Some(window) = handle.upgrade() {
            refresh_connections(&window, &backend);
        }
    });
}

/// Escolha da conexão que vira bloco no workspace. A lista é a mesma do cofre,
/// filtrada só pela busca do próprio seletor.
fn bind_connection_picker(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    let searching = Arc::clone(&backend);
    window.on_picker_query_changed(move |_| {
        if let Some(window) = handle.upgrade() {
            refresh_picker(&window, &searching);
        }
    });

    let handle = window.as_weak();
    window.on_picker_picked(move |id| {
        if let Some(window) = handle.upgrade() {
            window.invoke_connection_connect_requested(id);
        }
    });
    let _ = backend;
}

pub fn refresh_picker(window: &AppWindow, backend: &Backend) {
    let Ok(profiles) = backend.connections() else {
        return;
    };
    let query = window.get_picker_query().to_lowercase();

    let rows = profiles
        .iter()
        .filter(|profile| matches_query(profile, &query))
        .map(to_row)
        .collect::<Vec<_>>();
    window.set_picker_rows(ModelRc::new(VecModel::from(rows)));
}

/// Clicar num cartão abre o painel de detalhes com as ações do perfil.
fn bind_connection_detail(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_connection_opened(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Ok(profile) = backend.connection(&id) else {
            return;
        };
        window.set_detail_row(to_row(&profile));
        window.set_detail_open(true);
    });
}
