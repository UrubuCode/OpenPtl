use std::collections::BTreeSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    diff_snapshot, HlcClock, LogicalSnapshot, MutationBatch, MutationState, RemoteSnapshot,
};
use crate::constants::MUTATION_SCHEMA_VERSION;

/// Diário local do dispositivo. Fica cifrado em `mutations.bin` e nunca sobe
/// para o Drive: o que trafega são os lotes, não este arquivo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationStore {
    pub version: u32,
    /// Identidade deste aparelho. Gerada uma vez e nunca reaproveitada em
    /// outro, porque é ela que desempata marcas iguais.
    pub device_id: Uuid,
    #[serde(default)]
    pub clock: HlcClock,
    #[serde(default)]
    pub state: MutationState,
    /// Lotes já aplicados localmente e ainda não enviados. É o que torna o
    /// aplicativo utilizável offline.
    #[serde(default)]
    pub pending: Vec<MutationBatch>,
    /// Arquivos do Drive já processados, por id. O id é estável, então o nome
    /// do arquivo pode ser opaco e ainda assim ninguém baixa duas vezes.
    #[serde(default)]
    pub applied_files: BTreeSet<String>,
    /// Lotes já incorporados, por id. Protege contra o mesmo lote chegar em
    /// dois arquivos distintos, o que acontece quando um envio é repetido
    /// depois de a resposta se perder.
    #[serde(default)]
    pub applied_mutations: BTreeSet<Uuid>,
    #[serde(default)]
    pub base_snapshot: Option<Uuid>,
}

impl Default for MutationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MutationStore {
    pub fn new() -> Self {
        Self {
            version: MUTATION_SCHEMA_VERSION,
            device_id: Uuid::new_v4(),
            clock: HlcClock::default(),
            state: MutationState::default(),
            pending: Vec::new(),
            applied_files: BTreeSet::new(),
            applied_mutations: BTreeSet::new(),
            base_snapshot: None,
        }
    }

    /// Registra o que mudou desde a última chamada. Devolve `None` quando nada
    /// mudou — chamar isto a cada gravação do cofre é barato por isso.
    pub fn record_local(&mut self, snapshot: &LogicalSnapshot) -> Option<&MutationBatch> {
        let now = Utc::now().timestamp();
        let device = self.device_id;
        let ops = diff_snapshot(&self.state, snapshot, &mut self.clock, device, now);
        if ops.is_empty() {
            return None;
        }

        self.state.apply_all(&ops);
        let stamp = super::Stamp::new(self.clock.last(), device);
        let batch = MutationBatch::new(device, stamp, self.base_snapshot, ops);
        self.applied_mutations.insert(batch.mutation_id);
        self.pending.push(batch);
        self.pending.last()
    }

    /// Incorpora um lote vindo de outro aparelho. Devolve `false` quando ele
    /// já era conhecido.
    pub fn ingest(&mut self, batch: &MutationBatch) -> bool {
        if !self.applied_mutations.insert(batch.mutation_id) {
            return false;
        }
        // Adiantar o relógio antes de aplicar é o que dá causalidade: uma
        // edição local feita depois disto fica necessariamente à frente.
        self.clock.observe(batch.stamp.hlc, Utc::now().timestamp());
        self.state.apply_all(&batch.ops);
        true
    }

    /// Adota um snapshot remoto como base. Usado por um aparelho novo ou por
    /// um que ficou atrás do ponto de compactação.
    pub fn adopt(&mut self, snapshot: &RemoteSnapshot) {
        self.state = snapshot.state.clone();
        self.base_snapshot = Some(snapshot.snapshot_id);
        self.applied_mutations.extend(snapshot.covered.iter());
        if let Some(stamp) = self.state.max_stamp() {
            self.clock.observe(stamp.hlc, Utc::now().timestamp());
        }
        // Os lotes locais ainda não enviados continuam válidos: eles têm marca
        // própria e o LWW os reconcilia contra o snapshot.
        let pending = std::mem::take(&mut self.pending);
        for batch in &pending {
            self.state.apply_all(&batch.ops);
        }
        self.pending = pending;
    }

    /// Marca um lote como enviado, guardando o id do arquivo criado no Drive
    /// para que a próxima leitura não o baixe de volta.
    pub fn mark_pushed(&mut self, mutation_id: Uuid, file_id: &str) {
        self.applied_files.insert(file_id.to_string());
        self.pending
            .retain(|batch| batch.mutation_id != mutation_id);
    }

    pub fn mark_file_seen(&mut self, file_id: &str) {
        self.applied_files.insert(file_id.to_string());
    }

    pub fn has_seen_file(&self, file_id: &str) -> bool {
        self.applied_files.contains(file_id)
    }

    pub fn snapshot_now(&self) -> RemoteSnapshot {
        RemoteSnapshot {
            schema_version: MUTATION_SCHEMA_VERSION,
            snapshot_id: Uuid::now_v7(),
            device_id: self.device_id,
            created_at: Utc::now().timestamp(),
            state: self.state.clone(),
            covered: self.applied_mutations.iter().copied().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::mutations::EntityKind;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone)]
    struct Host {
        id: String,
        name: String,
    }

    fn snapshot_of(names: &[(&str, &str)]) -> LogicalSnapshot {
        let mut snapshot = LogicalSnapshot::new(vec![EntityKind::Connection]);
        for (id, name) in names {
            let host = Host {
                id: (*id).to_string(),
                name: (*name).to_string(),
            };
            snapshot.insert(EntityKind::Connection, id, &host).unwrap();
        }
        snapshot
    }

    #[test]
    fn a_second_identical_save_queues_nothing() {
        let mut store = MutationStore::new();
        let snapshot = snapshot_of(&[("a", "srv")]);
        assert!(store.record_local(&snapshot).is_some());
        assert!(store.record_local(&snapshot).is_none());
        assert_eq!(store.pending.len(), 1);
    }

    #[test]
    fn the_same_batch_is_never_applied_twice() {
        let mut source = MutationStore::new();
        source.record_local(&snapshot_of(&[("a", "srv")]));
        let batch = source.pending[0].clone();

        let mut target = MutationStore::new();
        assert!(target.ingest(&batch));
        assert!(!target.ingest(&batch));
    }

    #[test]
    fn two_devices_converge_regardless_of_exchange_order() {
        let mut left = MutationStore::new();
        let mut right = MutationStore::new();

        left.record_local(&snapshot_of(&[("a", "esquerda")]));
        right.record_local(&snapshot_of(&[("b", "direita")]));

        let from_left = left.pending.clone();
        let from_right = right.pending.clone();

        for batch in &from_right {
            left.ingest(batch);
        }
        for batch in from_left.iter().rev() {
            right.ingest(batch);
        }

        let mut left_ids: Vec<String> = left
            .state
            .live_records(EntityKind::Connection)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut right_ids: Vec<String> = right
            .state
            .live_records(EntityKind::Connection)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        left_ids.sort();
        right_ids.sort();

        assert_eq!(left_ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(left_ids, right_ids);
    }

    #[test]
    fn adopting_a_snapshot_keeps_local_work_that_was_never_sent() {
        let mut source = MutationStore::new();
        source.record_local(&snapshot_of(&[("a", "do-snapshot")]));
        let snapshot = source.snapshot_now();

        let mut target = MutationStore::new();
        target.record_local(&snapshot_of(&[("b", "local")]));
        target.adopt(&snapshot);

        let ids: Vec<String> = target
            .state
            .live_records(EntityKind::Connection)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
        assert_eq!(target.pending.len(), 1);
    }
}
