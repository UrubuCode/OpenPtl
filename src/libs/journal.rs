//! Registro de eventos da execução.
//!
//! Guarda o que o aplicativo fez para que uma falha possa ser entendida depois,
//! sem precisar reproduzir o problema com o log ligado. Vive em memória: nada
//! vai para disco, porque as mensagens podem citar host e usuário.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::Local;

/// Quantos eventos ficam guardados. Passando disso, o mais antigo sai — um
/// registro que cresce sem limite acaba comendo a memória de uma sessão longa.
const CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warning => "aviso",
            Level::Error => "erro",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub time: String,
    pub level: Level,
    pub message: String,
}

/// Diário compartilhado entre o domínio e a interface.
#[derive(Clone, Default)]
pub struct Journal {
    entries: Arc<Mutex<VecDeque<Entry>>>,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn info(&self, message: impl Into<String>) {
        self.push(Level::Info, message.into());
    }

    pub fn warning(&self, message: impl Into<String>) {
        self.push(Level::Warning, message.into());
    }

    pub fn error(&self, message: impl Into<String>) {
        self.push(Level::Error, message.into());
    }

    /// Do mais recente para o mais antigo, que é a ordem em que se procura uma
    /// falha que acabou de acontecer.
    pub fn snapshot(&self) -> Vec<Entry> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        entries.iter().rev().cloned().collect()
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    fn push(&self, level: Level, message: String) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(Entry {
            time: Local::now().format("%H:%M:%S").to_string(),
            level,
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_entry_comes_first() {
        let journal = Journal::new();
        journal.info("primeira");
        journal.info("segunda");

        let entries = journal.snapshot();
        assert_eq!(entries[0].message, "segunda");
        assert_eq!(entries[1].message, "primeira");
    }

    #[test]
    fn the_oldest_entry_is_dropped_at_capacity() {
        let journal = Journal::new();
        for index in 0..CAPACITY + 10 {
            journal.info(format!("evento {index}"));
        }

        assert_eq!(journal.snapshot().len(), CAPACITY);
        let entries = journal.snapshot();
        assert_eq!(entries[0].message, format!("evento {}", CAPACITY + 9));
        assert!(!entries.iter().any(|entry| entry.message == "evento 0"));
    }

    #[test]
    fn levels_keep_their_label() {
        let journal = Journal::new();
        journal.error("falhou");
        journal.warning("cuidado");

        let entries = journal.snapshot();
        assert_eq!(entries[0].level.label(), "aviso");
        assert_eq!(entries[1].level.label(), "erro");
    }

    #[test]
    fn clearing_empties_the_journal() {
        let journal = Journal::new();
        journal.info("algo");
        journal.clear();

        assert!(journal.snapshot().is_empty());
    }

    #[test]
    fn every_entry_gets_a_timestamp() {
        let journal = Journal::new();
        journal.info("algo");

        let entry = journal.snapshot().remove(0);
        assert_eq!(entry.time.len(), 8, "formato HH:MM:SS");
    }
}
