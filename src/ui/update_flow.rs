//! Consulta, download e abertura do instalador de atualizações.
//!
//! O download só é aceito depois que a assinatura minisign confere, e a
//! instalação em si é sempre um clique explícito: nada é instalado sozinho.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use slint::{ComponentHandle, SharedString};

use super::AppWindow;
use crate::backend::Backend;
use crate::libs::updater::Channel;

thread_local! {
    /// Instalador já baixado e verificado nesta sessão, se houver.
    static DOWNLOADED: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    refresh_channel(window, &backend);

    let handle = window.as_weak();
    let checking = Arc::clone(&backend);
    window.on_update_check_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        window.set_update_status("Procurando…".into());

        let deliver = handle.clone();
        checking.update_check(checking.update_channel(), move |outcome| {
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(availability) if availability.available => window.set_update_status(
                    format!(
                        "Versão {} disponível. Instalada: {}.",
                        availability.latest, availability.current
                    )
                    .into(),
                ),
                Ok(availability) => window.set_update_status(
                    format!("Você já está na versão {}.", availability.current).into(),
                ),
                Err(error) => window.set_update_status(format!("{error}").into()),
            });
        });
    });

    let handle = window.as_weak();
    let downloading = Arc::clone(&backend);
    window.on_update_download_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        window.set_update_status("Baixando e conferindo a assinatura…".into());
        window.set_update_ready(false);

        let deliver = handle.clone();
        downloading.update_download(downloading.update_channel(), move |outcome| {
            let _ = deliver.upgrade_in_event_loop(move |window| match outcome {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    DOWNLOADED.with(|slot| *slot.borrow_mut() = Some(path));
                    window.set_update_status(
                        format!("Assinatura conferida. {name} pronto para instalar.").into(),
                    );
                    window.set_update_ready(true);
                }
                Err(error) => {
                    DOWNLOADED.with(|slot| *slot.borrow_mut() = None);
                    window.set_update_ready(false);
                    window.set_update_status(format!("{error}").into());
                }
            });
        });
    });

    let handle = window.as_weak();
    let installing = Arc::clone(&backend);
    window.on_update_install_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let Some(path) = DOWNLOADED.with(|slot| slot.borrow().clone()) else {
            window.set_update_status("Baixe a atualização antes de instalar.".into());
            return;
        };

        match installing.update_open(&path) {
            Ok(()) => window.set_update_status("Instalador aberto. Conclua por ele.".into()),
            Err(error) => window.set_update_status(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    window.on_update_channel_changed(move |canary| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let channel = if canary {
            Channel::Canary
        } else {
            Channel::Stable
        };

        match backend.set_update_channel(channel) {
            Ok(()) => {
                // Trocar de canal invalida o que foi baixado do canal anterior.
                DOWNLOADED.with(|slot| *slot.borrow_mut() = None);
                window.set_update_ready(false);
                window.set_update_status(SharedString::new());
                refresh_channel(&window, &backend);
            }
            Err(error) => window.set_update_status(format!("{error}").into()),
        }
    });
}

fn refresh_channel(window: &AppWindow, backend: &Backend) {
    window.set_update_canary(backend.update_channel() == Channel::Canary);
}
