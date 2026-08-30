//! Publicação do andamento da sincronização.
//!
//! No frontend Tauri isso eram dois eventos, `sync:status` e `sync:progress`,
//! emitidos por um `AppHandle` que precisava ser carregado por toda a pilha de
//! sincronização. Aqui o estado fica num registro compartilhado que a interface
//! lê num temporizador, do mesmo jeito que a fila de transferências.

use std::sync::{Arc, Mutex};

use crate::libs::models::{BackendMessage, SyncState};

/// Andamento de uma etapa longa, como o envio de vários arquivos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub label: String,
    pub current: u32,
    pub total: u32,
}

impl Progress {
    pub fn percent(&self) -> u8 {
        if self.total == 0 {
            return 0;
        }
        let ratio = self.current.min(self.total) as f64 / self.total as f64;
        (ratio * 100.0).round() as u8
    }
}

#[derive(Debug, Clone, Default)]
struct Snapshot {
    state: Option<SyncState>,
    progress: Option<Progress>,
}

/// Canal de mão única entre a sincronização e a interface.
#[derive(Clone, Default)]
pub struct Reporter {
    inner: Arc<Mutex<Snapshot>>,
}

impl Reporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self, state: SyncState) {
        self.update(|snapshot| snapshot.state = Some(state));
    }

    pub fn progress(&self, label: &str, current: u32, total: u32) {
        self.update(|snapshot| {
            snapshot.progress = Some(Progress {
                label: label.to_owned(),
                current,
                total,
            })
        });
    }

    /// Encerra a etapa em andamento. O estado final continua legível.
    pub fn clear_progress(&self) {
        self.update(|snapshot| snapshot.progress = None);
    }

    pub fn state(&self) -> Option<SyncState> {
        self.read(|snapshot| snapshot.state.clone())
    }

    pub fn current_progress(&self) -> Option<Progress> {
        self.read(|snapshot| snapshot.progress.clone())
    }

    /// Mensagem exibível: o rótulo da etapa quando há uma, senão o estado.
    pub fn message(&self) -> BackendMessage {
        self.read(|snapshot| match (&snapshot.progress, &snapshot.state) {
            (Some(progress), _) => BackendMessage::from(progress.label.clone()),
            (None, Some(state)) => state.message.clone(),
            (None, None) => BackendMessage::from(String::new()),
        })
    }

    fn update(&self, apply: impl FnOnce(&mut Snapshot)) {
        if let Ok(mut snapshot) = self.inner.lock() {
            apply(&mut snapshot);
        }
    }

    fn read<T: Default>(&self, apply: impl FnOnce(&Snapshot) -> T) -> T {
        self.inner
            .lock()
            .map(|snapshot| apply(&snapshot))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(status: &str) -> SyncState {
        SyncState {
            connected: false,
            status: status.to_owned(),
            message: BackendMessage::from("etapa".to_string()),
            last_sync_at: None,
            pending_user_code: None,
            verification_url: None,
        }
    }

    #[test]
    fn percent_is_zero_without_a_total() {
        let progress = Progress {
            label: "enviando".into(),
            current: 3,
            total: 0,
        };
        assert_eq!(progress.percent(), 0);
    }

    #[test]
    fn percent_tracks_the_step() {
        let progress = Progress {
            label: "enviando".into(),
            current: 3,
            total: 4,
        };
        assert_eq!(progress.percent(), 75);
    }

    #[test]
    fn last_status_wins() {
        let reporter = Reporter::new();
        reporter.status(running("running"));
        reporter.status(running("done"));

        assert_eq!(reporter.state().unwrap().status, "done");
    }

    #[test]
    fn progress_label_takes_precedence_over_status() {
        let reporter = Reporter::new();
        reporter.status(running("running"));
        reporter.progress("enviando 2 de 5", 2, 5);

        assert_eq!(reporter.message().message, "enviando 2 de 5");

        reporter.clear_progress();
        assert_eq!(reporter.message().message, "etapa");
    }

    #[test]
    fn clearing_progress_keeps_the_state() {
        let reporter = Reporter::new();
        reporter.status(running("done"));
        reporter.progress("enviando", 1, 2);
        reporter.clear_progress();

        assert!(reporter.current_progress().is_none());
        assert_eq!(reporter.state().unwrap().status, "done");
    }
}
