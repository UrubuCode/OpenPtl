//! Registro de transferências SFTP em andamento.
//!
//! O worker que move os bytes atualiza o progresso aqui; a interface lê num
//! temporizador. Assim uma transferência de milhares de blocos não acorda o
//! event loop a cada bloco.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Download,
    Upload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Running,
    Done,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Transfer {
    pub id: String,
    pub name: String,
    pub direction: Direction,
    pub total: u64,
    pub transferred: u64,
    pub state: State,
}

impl Transfer {
    /// Progresso de 0 a 100. Tamanho desconhecido fica em zero em vez de
    /// fingir uma fração que não existe.
    pub fn percent(&self) -> u8 {
        if self.total == 0 {
            return match self.state {
                State::Done => 100,
                _ => 0,
            };
        }
        let ratio = self.transferred.min(self.total) as f64 / self.total as f64;
        (ratio * 100.0).round() as u8
    }

    pub fn finished(&self) -> bool {
        !matches!(self.state, State::Running)
    }
}

/// Fila compartilhada entre o runtime e a interface.
#[derive(Clone, Default)]
pub struct Registry {
    entries: Arc<Mutex<HashMap<String, Transfer>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self, name: &str, direction: Direction, total: u64) -> String {
        let id = Uuid::new_v4().to_string();
        let transfer = Transfer {
            id: id.clone(),
            name: name.to_owned(),
            direction,
            total,
            transferred: 0,
            state: State::Running,
        };

        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(id.clone(), transfer);
        }
        id
    }

    pub fn advance(&self, id: &str, bytes: u64) {
        self.update(id, |transfer| {
            transfer.transferred = transfer.transferred.saturating_add(bytes);
        });
    }

    pub fn finish(&self, id: &str, outcome: Result<u64, String>) {
        self.update(id, |transfer| match outcome {
            Ok(total) => {
                // O tamanho real só é conhecido no fim quando o servidor não o
                // informa antes; alinhar aqui evita barra parada em 99%.
                transfer.total = transfer.total.max(total);
                transfer.transferred = transfer.total;
                transfer.state = State::Done;
            }
            Err(message) => transfer.state = State::Failed(message),
        });
    }

    /// Instantâneo ordenado: em andamento primeiro, depois por nome.
    pub fn snapshot(&self) -> Vec<Transfer> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };

        let mut list: Vec<Transfer> = entries.values().cloned().collect();
        list.sort_by(|left, right| {
            left.finished()
                .cmp(&right.finished())
                .then(left.name.cmp(&right.name))
        });
        list
    }

    pub fn clear_finished(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, transfer| !transfer.finished());
        }
    }

    fn update(&self, id: &str, apply: impl FnOnce(&mut Transfer)) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(transfer) = entries.get_mut(id) {
                apply(transfer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_tracks_progress() {
        let registry = Registry::new();
        let id = registry.start("dump.sql", Direction::Download, 1000);

        registry.advance(&id, 250);
        assert_eq!(registry.snapshot()[0].percent(), 25);

        registry.advance(&id, 750);
        assert_eq!(registry.snapshot()[0].percent(), 100);
    }

    #[test]
    fn unknown_size_never_fakes_progress() {
        let registry = Registry::new();
        let id = registry.start("stream.bin", Direction::Download, 0);

        registry.advance(&id, 4096);
        assert_eq!(registry.snapshot()[0].percent(), 0);

        registry.finish(&id, Ok(4096));
        let done = registry.snapshot().remove(0);
        assert_eq!(done.percent(), 100);
        assert_eq!(done.total, 4096);
    }

    #[test]
    fn failure_keeps_the_message() {
        let registry = Registry::new();
        let id = registry.start("dump.sql", Direction::Upload, 10);

        registry.finish(&id, Err("permissão negada".into()));
        let failed = registry.snapshot().remove(0);
        assert_eq!(failed.state, State::Failed("permissão negada".into()));
        assert!(failed.finished());
    }

    #[test]
    fn running_transfers_come_first() {
        let registry = Registry::new();
        let done = registry.start("a-pronta", Direction::Download, 1);
        registry.start("z-em-curso", Direction::Download, 1);
        registry.finish(&done, Ok(1));

        assert_eq!(registry.snapshot()[0].name, "z-em-curso");
    }

    #[test]
    fn clearing_keeps_only_running_transfers() {
        let registry = Registry::new();
        let done = registry.start("pronta", Direction::Download, 1);
        registry.start("em-curso", Direction::Download, 1);
        registry.finish(&done, Ok(1));

        registry.clear_finished();
        let remaining = registry.snapshot();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "em-curso");
    }
}
