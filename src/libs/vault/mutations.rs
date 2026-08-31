use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::*;
use crate::libs::models::NotesBinPayload;
use crate::libs::mutations::{
    EntityKind, LogicalSnapshot, MutationBatch, MutationStore, RemoteHeader, RemoteSnapshot,
};

/// Domínios que atravessam dispositivos.
///
/// `window_state` e os metadados de sincronia ficam de fora de propósito: são
/// estado do aparelho, e sincronizá-los faria o celular arrastar a janela do
/// desktop.
const SYNCED_KINDS: [EntityKind; 6] = [
    EntityKind::Connection,
    EntityKind::Keychain,
    EntityKind::Note,
    EntityKind::AuthServer,
    EntityKind::Settings,
    EntityKind::KnownHosts,
];

/// Conteúdo do known_hosts como registro de instância única.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct KnownHostsRecord {
    #[serde(default)]
    content: String,
}

impl VaultManager {
    pub fn device_id(&self) -> Result<uuid::Uuid> {
        Ok(self.read_mutation_store()?.device_id)
    }

    /// O diário usa o mesmo envelope dos objetos remotos, com corpo JSON.
    ///
    /// O resto do cofre é bincode, mas o mapa CRDT guarda valores de campo
    /// como `serde_json::Value`, que um formato posicional não sabe ler de
    /// volta — precisa ser autodescritivo.
    pub(super) fn read_mutation_store(&self) -> Result<MutationStore> {
        if !self.mutations_path.exists() {
            return Ok(MutationStore::new());
        }
        let key = self.current_key()?;
        let bytes = fs::read(&self.mutations_path)
            .with_context(|| format!("Falha ao ler arquivo {}", self.mutations_path.display()))?;
        decrypt_remote_blob(&bytes, &key).context("Falha ao decodificar mutations.bin")
    }

    pub(super) fn write_mutation_store(&self, store: &MutationStore) -> Result<()> {
        let key = self.current_key()?;
        let bytes = encrypt_remote_blob(store, &key)?;
        if let Some(parent) = self.mutations_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Falha ao criar diretorio {}", parent.display()))?;
        }
        fs::write(&self.mutations_path, bytes).with_context(|| {
            format!(
                "Falha ao escrever arquivo {}",
                self.mutations_path.display()
            )
        })
    }

    /// Retrato lógico do que o cofre guarda agora.
    fn logical_snapshot(&self) -> Result<LogicalSnapshot> {
        let payload = self.payload()?;
        let mut snapshot = LogicalSnapshot::new(SYNCED_KINDS.to_vec());

        for profile in &payload.connections {
            snapshot.insert(EntityKind::Connection, &profile.id, profile)?;
        }
        for entry in &payload.keychain {
            snapshot.insert(EntityKind::Keychain, &entry.id, entry)?;
        }
        // Servidores vindos da lista oficial não entram no log: eles são
        // buscados no GitHub a cada login, e replicá-los faria a lista brigar
        // com a origem.
        for server in payload.auth_servers.iter().filter(|item| !item.from_remote) {
            snapshot.insert(EntityKind::AuthServer, &server.id, server)?;
        }
        snapshot.insert(EntityKind::Settings, "", &payload.settings)?;

        for note in &self.read_notes_store()?.notes {
            snapshot.insert(EntityKind::Note, &note.id, note)?;
        }

        let known_hosts = KnownHostsRecord {
            content: self.read_known_hosts_store().unwrap_or_default(),
        };
        snapshot.insert(EntityKind::KnownHosts, "", &known_hosts)?;

        Ok(snapshot)
    }

    /// Compara o cofre com o log e enfileira um lote quando algo mudou.
    ///
    /// A fila é local: a alteração já vale neste aparelho antes de qualquer
    /// rede. Enviar primeiro e só então aplicar deixaria o aplicativo
    /// inutilizável offline, sem ganhar segurança nenhuma — o Drive não sabe
    /// rejeitar um envio conflitante.
    pub(super) fn capture_mutations(&mut self) -> Result<()> {
        if self.runtime.materializing || !self.runtime.unlocked {
            return Ok(());
        }

        let snapshot = self.logical_snapshot()?;
        let mut store = self.read_mutation_store()?;
        if store.record_local(&snapshot).is_none() {
            return Ok(());
        }
        self.write_mutation_store(&store)
    }

    /// Reescreve o cofre a partir do estado do log. É o caminho de volta:
    /// tudo o que chega de outro aparelho passa por aqui.
    pub(super) fn materialize_from_store(&mut self, store: &MutationStore) -> Result<()> {
        self.assert_unlocked()?;
        self.runtime.materializing = true;
        let outcome = self.materialize_inner(store);
        self.runtime.materializing = false;
        outcome?;
        self.write_mutation_store(store)
    }

    fn materialize_inner(&mut self, store: &MutationStore) -> Result<()> {
        let state = &store.state;

        let mut connections = Vec::new();
        for (id, object) in state.live_records(EntityKind::Connection) {
            let mut profile: ConnectionProfile =
                record_into(ConnectionProfile::default(), &id, &object)?;
            profile.normalize_protocols();
            connections.push(profile);
        }

        let mut keychain = Vec::new();
        for (id, object) in state.live_records(EntityKind::Keychain) {
            keychain.push(record_into::<KeychainEntry>(
                KeychainEntry::default(),
                &id,
                &object,
            )?);
        }

        let mut local_servers = Vec::new();
        for (id, object) in state.live_records(EntityKind::AuthServer) {
            local_servers.push(record_into::<AuthServer>(
                AuthServer::default_server(),
                &id,
                &object,
            )?);
        }

        let mut notes = Vec::new();
        for (id, object) in state.live_records(EntityKind::Note) {
            notes.push(record_into::<Note>(Note::default(), &id, &object)?);
        }

        let settings = match state.singleton(EntityKind::Settings) {
            Some(object) => record_into::<AppSettings>(AppSettings::default(), "", &object)?,
            None => self.payload()?.settings.clone(),
        };

        let known_hosts = match state.singleton(EntityKind::KnownHosts) {
            Some(object) => {
                record_into::<KnownHostsRecord>(KnownHostsRecord::default(), "", &object)?.content
            }
            None => self.read_known_hosts_store().unwrap_or_default(),
        };

        {
            let payload = self.payload_mut()?;
            payload.connections = connections;
            payload.keychain = keychain;
            payload.settings = settings;
            // Os servidores oficiais são preservados: eles não vivem no log.
            let mut servers: Vec<AuthServer> = payload
                .auth_servers
                .iter()
                .filter(|item| item.from_remote)
                .cloned()
                .collect();
            servers.extend(local_servers);
            payload.auth_servers = servers;
            ensure_default_server(&mut payload.auth_servers);
        }

        self.write_notes_store(&NotesBinPayload {
            version: CURRENT_PAYLOAD_VERSION,
            notes,
        })?;
        self.write_known_hosts_store(&known_hosts)?;
        self.write_known_hosts_file(&known_hosts)?;
        self.persist()
    }

    /// Lotes ainda não enviados, na ordem em que foram gerados.
    pub fn pending_batches(&self) -> Result<Vec<MutationBatch>> {
        Ok(self.read_mutation_store()?.pending)
    }

    pub fn mutation_state_seen_file(&self, file_id: &str) -> Result<bool> {
        Ok(self.read_mutation_store()?.has_seen_file(file_id))
    }

    /// Confirma o envio de um lote e guarda o id do arquivo criado no Drive.
    pub fn confirm_pushed(&mut self, mutation_id: uuid::Uuid, file_id: &str) -> Result<()> {
        let mut store = self.read_mutation_store()?;
        store.mark_pushed(mutation_id, file_id);
        self.write_mutation_store(&store)
    }

    /// Incorpora lotes recebidos e reescreve o cofre quando algum era novo.
    pub fn ingest_remote(
        &mut self,
        batches: &[(String, MutationBatch)],
        snapshot: Option<(String, RemoteSnapshot)>,
    ) -> Result<bool> {
        let mut store = self.read_mutation_store()?;
        let mut changed = false;

        if let Some((file_id, snapshot)) = snapshot {
            if !store.has_seen_file(&file_id) {
                store.adopt(&snapshot);
                store.mark_file_seen(&file_id);
                changed = true;
            }
        }

        for (file_id, batch) in batches {
            if store.has_seen_file(file_id) {
                continue;
            }
            changed |= store.ingest(batch);
            store.mark_file_seen(file_id);
        }

        if !changed {
            self.write_mutation_store(&store)?;
            return Ok(false);
        }

        self.materialize_from_store(&store)?;
        Ok(true)
    }

    /// Estado completo para publicar como snapshot de compactação.
    pub fn snapshot_for_compaction(&self) -> Result<RemoteSnapshot> {
        Ok(self.read_mutation_store()?.snapshot_now())
    }

    pub fn adopt_compaction(&mut self, snapshot: &RemoteSnapshot, file_id: &str) -> Result<()> {
        let mut store = self.read_mutation_store()?;
        store.base_snapshot = Some(snapshot.snapshot_id);
        store.mark_file_seen(file_id);
        self.write_mutation_store(&store)
    }

    /// Semeia o log com tudo o que já existe no cofre. Usado quando o
    /// aparelho tem dados mas nunca sincronizou.
    pub fn seed_mutations(&mut self) -> Result<()> {
        self.capture_mutations()
    }

    /// Tudo o que uma rodada de sincronia precisa, lido de uma vez só.
    ///
    /// A rede nunca roda com o cadeado do cofre na mão: a fachada pega este
    /// retrato, solta o cadeado e só volta a ele para aplicar o resultado.
    pub fn sync_context(&self) -> Result<SyncContext> {
        let store = self.read_mutation_store()?;
        Ok(SyncContext {
            key: self.current_key()?,
            seen: store.applied_files.clone(),
            base_snapshot: store.base_snapshot,
            pending: store.pending.clone(),
            header: self.remote_header()?,
        })
    }

    /// Cabeçalho publicado no Drive. Não carrega segredo — salt e verificador
    /// da chave — mas sem ele um aparelho novo não deriva a mesma chave.
    pub fn remote_header(&self) -> Result<RemoteHeader> {
        let key = self.current_key()?;
        Ok(RemoteHeader {
            schema_version: crate::constants::MUTATION_SCHEMA_VERSION,
            salt: self.runtime.salt,
            key_check: compute_key_check(&key),
            created_at: self
                .runtime
                .created_at
                .unwrap_or_else(|| Utc::now().timestamp()),
        })
    }

    /// Confere a senha mestre contra o cabeçalho remoto, antes de baixar
    /// qualquer coisa.
    pub fn password_matches_header(header: &RemoteHeader, password: &str) -> Result<bool> {
        let salt = header
            .salt
            .ok_or_else(|| anyhow!("Cofre remoto usa chave do sistema e exige senha mestre"))?;
        let key = derive_key(password.trim(), &salt)?;
        Ok(compute_key_check(&key) == header.key_check)
    }
}

/// Retrato do estado de sincronia, tirado sob o cadeado e usado fora dele.
#[derive(Debug, Clone)]
pub struct SyncContext {
    pub key: [u8; 32],
    pub seen: std::collections::BTreeSet<String>,
    pub base_snapshot: Option<uuid::Uuid>,
    pub pending: Vec<MutationBatch>,
    pub header: RemoteHeader,
}

/// Monta o registro sobre um valor padrão para que campos ausentes — ou
/// anulados por uma mutação — não impeçam a desserialização.
fn record_into<T: DeserializeOwned + serde::Serialize>(
    base: T,
    id: &str,
    object: &Map<String, Value>,
) -> Result<T> {
    let mut merged = match serde_json::to_value(&base)? {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    for (field, value) in object {
        if value.is_null() {
            continue;
        }
        merged.insert(field.clone(), value.clone());
    }
    if merged.contains_key("id") {
        merged.insert("id".to_string(), Value::String(id.to_string()));
    }

    serde_json::from_value(Value::Object(merged))
        .with_context(|| format!("Falha ao materializar registro {id}"))
}
