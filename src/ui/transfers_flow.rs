//! Download e upload de arquivos por SFTP.
//!
//! A escolha do arquivo local usa o diálogo nativo do sistema. Ele é modal por
//! natureza, então roda na própria thread da interface; a transferência em si
//! vai para o runtime e só o progresso volta, lido num temporizador.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use super::{AppWindow, TransferRow};
use crate::backend::Backend;
use crate::libs::transfer::{Direction, State, Transfer};

/// Intervalo de atualização da barra de progresso.
const REFRESH_INTERVAL: Duration = Duration::from_millis(250);

pub fn bind(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let handle = window.as_weak();
    let downloading = Arc::clone(&backend);
    window.on_download_requested(move |remote| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        let suggested = file_name_of(&remote);
        let Some(target) = rfd::FileDialog::new().set_file_name(&suggested).save_file() else {
            return;
        };

        downloading.sftp_download(&session, &remote, target);
    });

    let handle = window.as_weak();
    let uploading = Arc::clone(&backend);
    window.on_upload_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(session) = active_session(&window) else {
            return;
        };

        let Some(source) = rfd::FileDialog::new().pick_file() else {
            return;
        };
        let Some(name) = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            window.set_files_message("Arquivo local sem nome utilizável.".into());
            return;
        };

        let remote = join(window.get_files_path().as_str(), &name);
        uploading.sftp_upload(&session, source, &remote);
    });

    let handle = window.as_weak();
    let clearing = Arc::clone(&backend);
    window.on_transfers_cleared(move || {
        clearing.transfers().clear_finished();
        if let Some(window) = handle.upgrade() {
            render(&window, &clearing);
        }
    });

    start_refresh(window, backend)
}

fn start_refresh(window: &AppWindow, backend: Arc<Backend>) -> Timer {
    let timer = Timer::default();
    let handle = window.as_weak();

    timer.start(TimerMode::Repeated, REFRESH_INTERVAL, move || {
        if let Some(window) = handle.upgrade() {
            render(&window, &backend);
        }
    });

    timer
}

fn render(window: &AppWindow, backend: &Backend) {
    let rows = backend
        .transfers()
        .snapshot()
        .iter()
        .map(to_row)
        .collect::<Vec<_>>();
    window.set_transfers(ModelRc::new(VecModel::from(rows)));
}

fn to_row(transfer: &Transfer) -> TransferRow {
    let failed = matches!(transfer.state, State::Failed(_));
    TransferRow {
        id: transfer.id.as_str().into(),
        name: transfer.name.as_str().into(),
        direction: match transfer.direction {
            Direction::Download => "↓".into(),
            Direction::Upload => "↑".into(),
        },
        progress: transfer.percent() as i32,
        detail: detail_of(transfer).into(),
        failed,
        done: transfer.finished() && !failed,
    }
}

/// Texto à direita da barra: a causa do erro quando falha, o progresso quando
/// está em curso.
fn detail_of(transfer: &Transfer) -> String {
    match &transfer.state {
        State::Failed(message) => message.clone(),
        State::Done => "concluído".to_owned(),
        State::Running => format!("{}%", transfer.percent()),
    }
}

fn active_session(window: &AppWindow) -> Option<String> {
    let session = window.get_active_session();
    (!session.is_empty()).then(|| session.to_string())
}

fn file_name_of(path: &SharedString) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path.as_str())
        .to_owned()
}

fn join(base: &str, child: &str) -> String {
    format!("{}/{child}", base.trim_end_matches('/'))
}
