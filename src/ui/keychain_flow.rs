//! Fluxo do chaveiro: listar, criar, editar e excluir entradas.
//!
//! Segredos só saem do cofre para preencher o formulário de edição; a lista
//! carrega apenas nome, tipo e data.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{AppWindow, KeychainDraft, KeychainRow, SelectOption};
use crate::backend::Backend;
use crate::libs::models::{KeychainEntry, KeychainEntryType};

const KIND_PASSWORD: &str = "password";
const KIND_SSH_KEY: &str = "ssh_key";
const KIND_SECRET: &str = "secret";

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_keychain_create_requested(move || {
        if let Some(window) = handle.upgrade() {
            open_form(&window, empty_draft());
        }
    });

    let handle = window.as_weak();
    let editing = Arc::clone(&backend);
    window.on_keychain_edit_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match editing.keychain_entry(&id) {
            Ok(entry) => open_form(&window, to_draft(&entry)),
            Err(error) => window.set_keychain_error(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    let deleting = Arc::clone(&backend);
    window.on_keychain_delete_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match deleting.keychain_delete(&id) {
            Ok(()) => refresh(&window, &deleting),
            Err(error) => window.set_keychain_error(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    window.on_keychain_form_dismissed(move || {
        if let Some(window) = handle.upgrade() {
            close_form(&window);
        }
    });

    let handle = window.as_weak();
    window.on_keychain_saved(move |draft| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.keychain_save(from_draft(&draft)) {
            Ok(_) => {
                close_form(&window);
                refresh(&window, &backend);
            }
            Err(error) => window.set_keychain_error(format!("{error}").into()),
        }
    });
}

pub fn refresh(window: &AppWindow, backend: &Backend) {
    match backend.keychain() {
        Ok(entries) => {
            let rows = entries.iter().map(to_row).collect::<Vec<_>>();
            window.set_keychain(ModelRc::new(VecModel::from(rows)));

            // A mesma lista alimenta a escolha de chave no formulario de
            // conexao, para o usuario escolher pelo nome em vez de digitar um
            // identificador.
            let options = entries
                .iter()
                .map(|entry| SelectOption {
                    id: entry.id.as_str().into(),
                    label: entry.name.as_str().into(),
                })
                .collect::<Vec<_>>();
            window.set_keychain_options(ModelRc::new(VecModel::from(options)));
        }
        Err(error) => window.set_keychain_error(format!("{error}").into()),
    }
}

fn open_form(window: &AppWindow, draft: KeychainDraft) {
    window.set_keychain_draft(draft);
    window.set_keychain_error(SharedString::new());
    window.set_keychain_form_open(true);
}

fn close_form(window: &AppWindow) {
    window.set_keychain_form_open(false);
    window.set_keychain_draft(empty_draft());
    window.set_keychain_error(SharedString::new());
}

fn to_row(entry: &KeychainEntry) -> KeychainRow {
    KeychainRow {
        id: entry.id.as_str().into(),
        name: entry.name.as_str().into(),
        kind: label_of(&entry.entry_type).into(),
        created_at: format_timestamp(entry.created_at).into(),
    }
}

fn to_draft(entry: &KeychainEntry) -> KeychainDraft {
    KeychainDraft {
        id: entry.id.as_str().into(),
        name: entry.name.as_str().into(),
        kind: kind_of(&entry.entry_type).into(),
        password: entry.password.clone().unwrap_or_default().into(),
        private_key: entry.private_key.clone().unwrap_or_default().into(),
        public_key: entry.public_key.clone().unwrap_or_default().into(),
        passphrase: entry.passphrase.clone().unwrap_or_default().into(),
    }
}

fn empty_draft() -> KeychainDraft {
    KeychainDraft {
        kind: KIND_PASSWORD.into(),
        ..Default::default()
    }
}

fn from_draft(draft: &KeychainDraft) -> KeychainEntry {
    let entry_type = match draft.kind.as_str() {
        KIND_SSH_KEY => KeychainEntryType::SshKey,
        KIND_SECRET => KeychainEntryType::Secret,
        _ => KeychainEntryType::Password,
    };

    KeychainEntry {
        id: draft.id.to_string(),
        name: draft.name.trim().to_string(),
        entry_type,
        password: optional(&draft.password),
        private_key: optional(&draft.private_key),
        public_key: optional(&draft.public_key),
        passphrase: optional(&draft.passphrase),
        created_at: 0,
    }
}

fn kind_of(entry_type: &KeychainEntryType) -> &'static str {
    match entry_type {
        KeychainEntryType::Password => KIND_PASSWORD,
        KeychainEntryType::SshKey => KIND_SSH_KEY,
        KeychainEntryType::Secret => KIND_SECRET,
    }
}

fn label_of(entry_type: &KeychainEntryType) -> &'static str {
    match entry_type {
        KeychainEntryType::Password => "Senha",
        KeychainEntryType::SshKey => "Chave SSH",
        KeychainEntryType::Secret => "Segredo",
    }
}

/// Data de criação em formato local legível; zero significa "sem registro".
fn format_timestamp(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    Utc.timestamp_opt(seconds, 0)
        .single()
        .map(|moment| moment.format("%d/%m/%Y").to_string())
        .unwrap_or_default()
}

fn optional(value: &SharedString) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
