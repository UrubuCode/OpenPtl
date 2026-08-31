use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{EntityKind, Op, Stamp};

/// Chave de um registro dentro do mapa CRDT. Texto porque o estado inteiro é
/// serializado em JSON, que não aceita chave composta.
pub fn record_key(entity: EntityKind, id: &str) -> String {
    format!("{}/{}", entity.as_str(), id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValue {
    pub value: Value,
    pub stamp: Stamp,
}

/// Um registro visto pelo log: cada campo com a marca de quem o escreveu por
/// último, mais a lápide de remoção, se houver.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordState {
    #[serde(default)]
    pub fields: BTreeMap<String, FieldValue>,
    #[serde(default)]
    pub deleted: Option<Stamp>,
}

impl RecordState {
    /// Um registro existe enquanto algum campo for mais novo que a lápide.
    /// Isso dá semântica de "reescrever vence a remoção": editar um host que o
    /// outro aparelho apagou o traz de volta, em vez de perder a edição.
    pub fn is_live(&self) -> bool {
        match self.deleted {
            None => !self.fields.is_empty(),
            Some(tomb) => self.fields.values().any(|field| field.stamp > tomb),
        }
    }

    /// Reconstrói o objeto, descartando campos anteriores à lápide.
    pub fn to_object(&self) -> Map<String, Value> {
        let mut object = Map::new();
        for (name, field) in &self.fields {
            if let Some(tomb) = self.deleted {
                if field.stamp <= tomb {
                    continue;
                }
            }
            object.insert(name.clone(), field.value.clone());
        }
        object
    }
}

/// O estado materializado do log. Aplicar as mesmas operações em qualquer
/// ordem leva sempre a este mesmo mapa — sem isso, dois aparelhos que
/// recebessem os lotes em ordens diferentes divergiriam.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MutationState {
    #[serde(default)]
    pub records: BTreeMap<String, RecordState>,
}

impl MutationState {
    pub fn apply(&mut self, op: &Op) {
        let key = record_key(op.entity(), op.id());
        let record = self.records.entry(key).or_default();

        match op {
            Op::Set {
                field,
                value,
                stamp,
                ..
            } => {
                let replace = record
                    .fields
                    .get(field)
                    .map(|existing| *stamp > existing.stamp)
                    .unwrap_or(true);
                if replace {
                    record.fields.insert(
                        field.clone(),
                        FieldValue {
                            value: value.clone(),
                            stamp: *stamp,
                        },
                    );
                }
            }
            Op::Delete { stamp, .. } => {
                let replace = record.deleted.map(|tomb| *stamp > tomb).unwrap_or(true);
                if replace {
                    record.deleted = Some(*stamp);
                }
            }
        }
    }

    pub fn apply_all<'a>(&mut self, ops: impl IntoIterator<Item = &'a Op>) {
        for op in ops {
            self.apply(op);
        }
    }

    /// Registros vivos de um domínio, em ordem estável por id.
    pub fn live_records(&self, entity: EntityKind) -> Vec<(String, Map<String, Value>)> {
        let prefix = format!("{}/", entity.as_str());
        self.records
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .filter(|(_, record)| record.is_live())
            .map(|(key, record)| {
                let id = key[prefix.len()..].to_string();
                (id, record.to_object())
            })
            .collect()
    }

    /// Objeto de um domínio de instância única, quando já foi escrito.
    pub fn singleton(&self, entity: EntityKind) -> Option<Map<String, Value>> {
        let record = self.records.get(&record_key(entity, ""))?;
        if !record.is_live() {
            return None;
        }
        Some(record.to_object())
    }

    /// Marca mais alta já vista. Usada para adiantar o relógio local depois de
    /// carregar um snapshot, que não traz os lotes que o originaram.
    pub fn max_stamp(&self) -> Option<Stamp> {
        self.records
            .values()
            .flat_map(|record| {
                record
                    .fields
                    .values()
                    .map(|field| field.stamp)
                    .chain(record.deleted)
            })
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::super::hlc::Hlc;
    use super::*;
    use uuid::Uuid;

    fn stamp(wall: i64, counter: u32, device: u128) -> Stamp {
        Stamp::new(Hlc { wall, counter }, Uuid::from_u128(device))
    }

    fn set(id: &str, field: &str, value: &str, stamp: Stamp) -> Op {
        Op::Set {
            entity: EntityKind::Connection,
            id: id.to_string(),
            field: field.to_string(),
            value: Value::String(value.to_string()),
            stamp,
        }
    }

    #[test]
    fn the_newest_stamp_wins_regardless_of_arrival_order() {
        let old = set("a", "host", "antigo", stamp(1, 0, 1));
        let new = set("a", "host", "novo", stamp(2, 0, 1));

        let mut forward = MutationState::default();
        forward.apply_all([&old, &new]);
        let mut backward = MutationState::default();
        backward.apply_all([&new, &old]);

        assert_eq!(
            forward.records["connection/a"].fields["host"].value,
            Value::String("novo".into())
        );
        assert_eq!(
            forward.records["connection/a"].fields["host"].value,
            backward.records["connection/a"].fields["host"].value
        );
    }

    #[test]
    fn two_devices_editing_different_fields_do_not_overwrite_each_other() {
        let mut state = MutationState::default();
        state.apply(&set("a", "host", "srv", stamp(1, 0, 1)));
        state.apply(&set("a", "username", "root", stamp(1, 0, 2)));

        let object = state.records["connection/a"].to_object();
        assert_eq!(object["host"], Value::String("srv".into()));
        assert_eq!(object["username"], Value::String("root".into()));
    }

    #[test]
    fn a_late_write_does_not_resurrect_an_older_delete() {
        let mut state = MutationState::default();
        state.apply(&set("a", "host", "srv", stamp(1, 0, 1)));
        state.apply(&Op::Delete {
            entity: EntityKind::Connection,
            id: "a".into(),
            stamp: stamp(5, 0, 1),
        });

        assert!(!state.records["connection/a"].is_live());

        // Uma escrita anterior à lápide chega atrasada e não deve reviver nada.
        state.apply(&set("a", "port", "22", stamp(3, 0, 2)));
        assert!(!state.records["connection/a"].is_live());
    }

    #[test]
    fn editing_after_a_delete_brings_the_record_back() {
        let mut state = MutationState::default();
        state.apply(&Op::Delete {
            entity: EntityKind::Connection,
            id: "a".into(),
            stamp: stamp(5, 0, 1),
        });
        state.apply(&set("a", "host", "srv", stamp(9, 0, 2)));

        assert!(state.records["connection/a"].is_live());
        let object = state.records["connection/a"].to_object();
        assert_eq!(object.len(), 1);
        assert_eq!(object["host"], Value::String("srv".into()));
    }

    #[test]
    fn the_device_decides_when_the_clocks_are_identical() {
        let mut state = MutationState::default();
        state.apply(&set("a", "host", "de-1", stamp(1, 0, 1)));
        state.apply(&set("a", "host", "de-2", stamp(1, 0, 2)));
        assert_eq!(
            state.records["connection/a"].fields["host"].value,
            Value::String("de-2".into())
        );
    }
}
