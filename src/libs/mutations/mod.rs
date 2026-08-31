//! Log de mutações: o formato que os dispositivos trocam entre si.
//!
//! Cada alteração local vira um lote imutável, identificado por UUID e datado
//! por relógio lógico. Aplicar o mesmo conjunto de lotes em qualquer ordem
//! produz o mesmo estado — é essa propriedade que permite sincronizar sem um
//! servidor que arbitre, já que o Drive não oferece compare-and-swap.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::MUTATION_SCHEMA_VERSION;

mod diff;
mod hlc;
mod state;
mod store;

pub use diff::{diff_snapshot, LogicalSnapshot};
pub use hlc::{HlcClock, Stamp};
pub use state::{record_key, MutationState};
pub use store::MutationStore;

/// Domínios versionados pelo log. O nome de cada variante entra na chave dos
/// registros, então renomear uma delas invalida o histórico já publicado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Connection,
    Keychain,
    Note,
    AuthServer,
    Settings,
    KnownHosts,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Connection => "connection",
            EntityKind::Keychain => "keychain",
            EntityKind::Note => "note",
            EntityKind::AuthServer => "auth_server",
            EntityKind::Settings => "settings",
            EntityKind::KnownHosts => "known_hosts",
        }
    }
}

/// Uma operação sobre um campo ou sobre um registro inteiro.
///
/// `Set` é granular por campo justamente para que dois dispositivos que editam
/// atributos diferentes do mesmo host não se sobrescrevam. `Delete` é lápide:
/// remover o registro de verdade faria uma mutação atrasada ressuscitá-lo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Set {
        entity: EntityKind,
        id: String,
        field: String,
        value: serde_json::Value,
        stamp: Stamp,
    },
    Delete {
        entity: EntityKind,
        id: String,
        stamp: Stamp,
    },
}

impl Op {
    pub fn entity(&self) -> EntityKind {
        match self {
            Op::Set { entity, .. } | Op::Delete { entity, .. } => *entity,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Op::Set { id, .. } | Op::Delete { id, .. } => id,
        }
    }
}

/// Um arquivo do log. É imutável: o nome dele no Drive é o `mutation_id` e
/// nunca é reescrito, o que dispensa qualquer trava no armazenamento remoto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationBatch {
    pub schema_version: u32,
    pub mutation_id: Uuid,
    pub device_id: Uuid,
    /// Marca do lote como um todo. As operações têm marcas próprias; esta
    /// serve para ordenar lotes na exibição e para avançar o relógio de quem
    /// recebe.
    pub stamp: Stamp,
    /// Snapshot sobre o qual o lote foi gerado, quando havia um.
    pub base_snapshot: Option<Uuid>,
    pub created_at: i64,
    pub ops: Vec<Op>,
}

impl MutationBatch {
    pub fn new(device_id: Uuid, stamp: Stamp, base_snapshot: Option<Uuid>, ops: Vec<Op>) -> Self {
        Self {
            schema_version: MUTATION_SCHEMA_VERSION,
            // Gerado uma única vez por lote e reaproveitado em toda
            // retentativa: o Drive não deduplica por nome, então reenviar com
            // um id novo criaria um segundo arquivo com o mesmo conteúdo.
            mutation_id: Uuid::now_v7(),
            device_id,
            stamp,
            base_snapshot,
            created_at: chrono::Utc::now().timestamp(),
            ops,
        }
    }

    /// Nome do arquivo remoto. Opaco de propósito: prefixar com o dispositivo
    /// entregaria ao Drive quantos aparelhos existem e qual muda mais.
    pub fn file_name(&self) -> String {
        format!("{}.bin", self.mutation_id)
    }
}

/// Estado completo publicado periodicamente para que o log possa ser podado e
/// para que um dispositivo novo não precise reproduzir o histórico inteiro.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSnapshot {
    pub schema_version: u32,
    pub snapshot_id: Uuid,
    pub device_id: Uuid,
    pub created_at: i64,
    pub state: MutationState,
    /// Lotes já incorporados: quem tem o snapshot pode ignorá-los.
    pub covered: Vec<Uuid>,
}

impl RemoteSnapshot {
    pub fn file_name(&self) -> String {
        format!(
            "{}{}.bin",
            crate::constants::REMOTE_SNAPSHOT_PREFIX,
            self.snapshot_id
        )
    }
}

/// Cabeçalho do cofre remoto. Guarda o salt e o verificador da chave mestre —
/// nenhum segredo — porque sem eles um dispositivo novo não consegue derivar a
/// mesma chave e portanto não abriria nenhum lote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHeader {
    pub schema_version: u32,
    pub salt: Option<[u8; 16]>,
    pub key_check: [u8; 32],
    pub created_at: i64,
}
