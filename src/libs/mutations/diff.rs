use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::{record_key, EntityKind, HlcClock, MutationState, Op, Stamp};

/// Retrato do que o cofre contém agora, já em forma de objeto JSON por
/// registro. É a entrada do diferenciador: comparar isto com o mapa CRDT diz
/// exatamente o que foi acrescentado, removido ou alterado.
#[derive(Debug, Clone, Default)]
pub struct LogicalSnapshot {
    records: BTreeMap<String, Map<String, Value>>,
    kinds: Vec<EntityKind>,
}

impl LogicalSnapshot {
    pub fn new(kinds: Vec<EntityKind>) -> Self {
        Self {
            records: BTreeMap::new(),
            kinds,
        }
    }

    /// Acrescenta um registro. O `id` sai do objeto: ele já é a chave, e
    /// mantê-lo dentro faria toda renomeação de id virar uma alteração de
    /// campo.
    pub fn insert<T: Serialize>(&mut self, entity: EntityKind, id: &str, value: &T) -> Result<()> {
        let mut object = match serde_json::to_value(value)? {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };
        object.remove("id");
        self.records.insert(record_key(entity, id), object);
        Ok(())
    }
}

/// Compara o retrato atual com o estado do log e devolve só o que mudou.
///
/// Cada operação recebe a própria marca, tirada do relógio na ordem em que é
/// gerada, para que campos alterados no mesmo instante ainda tenham ordem
/// estável entre si.
pub fn diff_snapshot(
    state: &MutationState,
    snapshot: &LogicalSnapshot,
    clock: &mut HlcClock,
    device: Uuid,
    now: i64,
) -> Vec<Op> {
    let mut ops = Vec::new();

    for (key, object) in &snapshot.records {
        let Some((entity, id)) = split_key(key, &snapshot.kinds) else {
            continue;
        };
        let current = state
            .records
            .get(key)
            .filter(|record| record.is_live())
            .map(|record| record.to_object())
            .unwrap_or_default();

        for (field, value) in object {
            if current.get(field) == Some(value) {
                continue;
            }
            ops.push(Op::Set {
                entity,
                id: id.clone(),
                field: field.clone(),
                value: value.clone(),
                stamp: Stamp::new(clock.tick(now), device),
            });
        }

        // Campo que sumiu do modelo vira nulo em vez de ficar preso no mapa;
        // a materialização descarta nulos antes de desserializar.
        for field in current.keys() {
            if object.contains_key(field) {
                continue;
            }
            ops.push(Op::Set {
                entity,
                id: id.clone(),
                field: field.clone(),
                value: Value::Null,
                stamp: Stamp::new(clock.tick(now), device),
            });
        }
    }

    for (key, record) in &state.records {
        if !record.is_live() || snapshot.records.contains_key(key) {
            continue;
        }
        let Some((entity, id)) = split_key(key, &snapshot.kinds) else {
            continue;
        };
        ops.push(Op::Delete {
            entity,
            id,
            stamp: Stamp::new(clock.tick(now), device),
        });
    }

    ops
}

/// Só devolve domínios que o retrato declara cobrir: sem isso, um retrato
/// parcial apagaria tudo o que ele não conhece.
fn split_key(key: &str, kinds: &[EntityKind]) -> Option<(EntityKind, String)> {
    let (prefix, id) = key.split_once('/')?;
    let entity = kinds.iter().copied().find(|kind| kind.as_str() == prefix)?;
    Some((entity, id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone)]
    struct Host {
        id: String,
        name: String,
        port: u16,
    }

    fn snapshot_of(hosts: &[Host]) -> LogicalSnapshot {
        let mut snapshot = LogicalSnapshot::new(vec![EntityKind::Connection]);
        for host in hosts {
            snapshot
                .insert(EntityKind::Connection, &host.id, host)
                .unwrap();
        }
        snapshot
    }

    fn host(id: &str, name: &str, port: u16) -> Host {
        Host {
            id: id.to_string(),
            name: name.to_string(),
            port,
        }
    }

    #[test]
    fn an_unchanged_snapshot_produces_no_operations() {
        let mut clock = HlcClock::default();
        let device = Uuid::from_u128(1);
        let mut state = MutationState::default();

        let snapshot = snapshot_of(&[host("a", "srv", 22)]);
        let first = diff_snapshot(&state, &snapshot, &mut clock, device, 10);
        state.apply_all(&first);
        assert!(!first.is_empty());

        let second = diff_snapshot(&state, &snapshot, &mut clock, device, 20);
        assert!(second.is_empty(), "esperado nenhum op, veio {second:?}");
    }

    #[test]
    fn only_the_changed_field_becomes_an_operation() {
        let mut clock = HlcClock::default();
        let device = Uuid::from_u128(1);
        let mut state = MutationState::default();

        state.apply_all(&diff_snapshot(
            &state.clone(),
            &snapshot_of(&[host("a", "srv", 22)]),
            &mut clock,
            device,
            10,
        ));

        let ops = diff_snapshot(
            &state,
            &snapshot_of(&[host("a", "srv", 2222)]),
            &mut clock,
            device,
            20,
        );
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Op::Set { field, value, .. } => {
                assert_eq!(field, "port");
                assert_eq!(value, &Value::from(2222));
            }
            other => panic!("esperado Set, veio {other:?}"),
        }
    }

    #[test]
    fn a_removed_record_becomes_a_tombstone() {
        let mut clock = HlcClock::default();
        let device = Uuid::from_u128(1);
        let mut state = MutationState::default();

        state.apply_all(&diff_snapshot(
            &state.clone(),
            &snapshot_of(&[host("a", "srv", 22)]),
            &mut clock,
            device,
            10,
        ));

        let ops = diff_snapshot(&state, &snapshot_of(&[]), &mut clock, device, 20);
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], Op::Delete { .. }));
    }
}
