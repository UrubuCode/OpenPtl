//! Fluxo dos hosts conhecidos: listar e revogar.
//!
//! Revogar não é apenas apagar uma linha do arquivo de trabalho: o conteúdo
//! precisa voltar ao armazenamento protegido, senão a revogação se perde no
//! próximo desbloqueio do cofre.

use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};

use super::{AppWindow, KnownHostRow};
use crate::backend::Backend;
use crate::libs::models::KnownHostEntry;

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_known_host_remove_requested(move |line_raw| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.known_host_remove(&line_raw) {
            Ok(()) => refresh(&window, &backend),
            Err(error) => window.set_status_message(format!("{error}").into()),
        }
    });
}

pub fn refresh(window: &AppWindow, backend: &Backend) {
    match backend.known_hosts() {
        Ok(entries) => {
            let path = entries
                .first()
                .map(|entry| entry.path.as_str())
                .unwrap_or_default();
            window.set_known_hosts_path(path.into());

            let rows = entries.iter().map(to_row).collect::<Vec<_>>();
            window.set_known_hosts(ModelRc::new(VecModel::from(rows)));
        }
        Err(error) => window.set_status_message(format!("{error}").into()),
    }
}

fn to_row(entry: &KnownHostEntry) -> KnownHostRow {
    KnownHostRow {
        host: entry.host.as_str().into(),
        port: entry.port.to_string().into(),
        key_type: entry.key_type.as_str().into(),
        fingerprint: entry.fingerprint.as_str().into(),
        line_raw: entry.line_raw.as_str().into(),
        path: entry.path.as_str().into(),
    }
}
