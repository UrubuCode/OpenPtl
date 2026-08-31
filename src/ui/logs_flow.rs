//! Página de logs.
//!
//! O diário vive no domínio; aqui só o traduzimos para linhas e reagimos ao
//! pedido de limpar.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use super::{AppWindow, LogRow};
use crate::backend::Backend;
use crate::libs::journal::Entry;

/// Cadência de leitura. O diário muda por ação do usuário, não continuamente.
const REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

pub fn bind(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let handle = window.as_weak();
    let clearing = Arc::clone(&backend);
    window.on_logs_cleared(move || {
        clearing.journal().clear();
        if let Some(window) = handle.upgrade() {
            refresh(&window, &clearing);
        }
    });

    let timer = Timer::default();
    let handle = window.as_weak();
    timer.start(TimerMode::Repeated, REFRESH_INTERVAL, move || {
        if let Some(window) = handle.upgrade() {
            refresh(&window, &backend);
        }
    });
    timer
}

fn refresh(window: &AppWindow, backend: &Backend) {
    let rows = backend
        .journal()
        .snapshot()
        .iter()
        .map(to_row)
        .collect::<Vec<_>>();
    window.set_logs(ModelRc::new(VecModel::from(rows)));
}

fn to_row(entry: &Entry) -> LogRow {
    LogRow {
        time: entry.time.as_str().into(),
        level: entry.level.label().into(),
        message: entry.message.as_str().into(),
    }
}
