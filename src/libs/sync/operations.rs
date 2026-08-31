use super::*;

impl SyncManager {
    pub fn new() -> Self {
        Self
    }

    pub fn clear_local_auth(&self) {
        delete_keyring_field(KEYRING_REFRESH_TOKEN);
        delete_keyring_field(KEYRING_USER_EMAIL);
        delete_keyring_field(KEYRING_USER_NAME);
        delete_keyring_field(KEYRING_USER_PICTURE);
    }

    pub async fn delete_remote_backup(
        &mut self,
        server_address: &str,
        fallback_addresses: &[String],
    ) -> Result<()> {
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        let client = Client::new();

        let Some(folder_id) =
            ensure_vault_folder(&client, &access_token, &vault_scope(), false).await?
        else {
            return Ok(());
        };

        for file in list_drive_files(&client, &access_token, &folder_id).await? {
            delete_drive_file(&client, &access_token, &file.id).await?;
        }

        delete_drive_file(&client, &access_token, &folder_id).await?;
        Ok(())
    }

    pub async fn google_login(
        &mut self,
        reporter: &Reporter,
        server_address: &str,
        client_id: Option<String>,
    ) -> Result<SyncState> {
        clear_sync_cancel();
        set_pending_client_id(client_id.clone());
        let pending = SyncState {
            connected: false,
            status: "running".to_string(),
            message: BackendMessage::key("sync_login_opening_browser"),
            last_sync_at: None,
            pending_user_code: None,
            verification_url: None,
        };
        reporter.status(pending.clone());

        // Callback por porta local em todas as plataformas. O frontend Tauri
        // usava deep link no Windows, que dependia de um plugin que não existe
        // mais aqui; o loopback é também o recomendado para apps desktop.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("Falha ao abrir porta local para callback")?;
        let local_port = listener.local_addr()?.port();
        let callback_url = format!("http://localhost:{}/callback", local_port);
        let login_url = match &client_id {
            Some(id) => format!(
                "{}/auth/google?local_callback={}&client_id={}",
                server_address,
                urlencoding::encode(&callback_url),
                urlencoding::encode(id)
            ),
            None => format!(
                "{}/auth/google?local_callback={}",
                server_address,
                urlencoding::encode(&callback_url)
            ),
        };

        open_login_url(&login_url)?;
        let result = tokio::select! {
            result = tokio::time::timeout(AUTH_CALLBACK_TIMEOUT, wait_for_callback(&listener)) => {
                finalize_auth_result(reporter, result)?
            }
            _ = wait_for_sync_cancel() => {
                let state = cancelled_state();
                reporter.status(state.clone());
                state
            }
        };
        Ok(result)
    }

    pub fn logged_user(&self) -> Option<SyncLoggedUser> {
        let email = load_user_field(KEYRING_USER_EMAIL)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let name = load_user_field(KEYRING_USER_NAME)
            .ok()
            .filter(|value| !value.trim().is_empty());
        let picture_url = load_user_field(KEYRING_USER_PICTURE)
            .ok()
            .filter(|value| !value.trim().is_empty());

        if email.is_none() && name.is_none() && picture_url.is_none() {
            return None;
        }

        Some(SyncLoggedUser {
            name,
            email,
            picture_url,
        })
    }

    /// Baixa o que ainda não foi visto e devolve para o cofre aplicar.
    ///
    /// Só os arquivos cujo id do Drive é desconhecido são baixados. O id é
    /// estável, então o nome pode ser opaco e ainda assim ninguém transfere o
    /// mesmo lote duas vezes.
    pub async fn fetch_remote(
        &mut self,
        reporter: &Reporter,
        key: &[u8; 32],
        seen: &BTreeSet<String>,
        base_snapshot: Option<Uuid>,
    ) -> Result<RemoteFetch> {
        let client = Client::new();
        let access_token = self.access_token().await?;

        let Some(folder_id) =
            ensure_vault_folder(&client, &access_token, &vault_scope(), false).await?
        else {
            return Ok(RemoteFetch::default());
        };

        let layout =
            RemoteLayout::classify(list_drive_files(&client, &access_token, &folder_id).await?);
        let mut fetch = RemoteFetch {
            remote_batch_count: layout.batches.len(),
            ..RemoteFetch::default()
        };

        // O snapshot vem primeiro: adotá-lo antes dos lotes evita reproduzir
        // um histórico que ele já resume.
        if let Some(meta) = layout.latest_snapshot() {
            let already_current = layout
                .latest_snapshot()
                .and_then(|file| snapshot_id_from_name(file.file_name()))
                .zip(base_snapshot)
                .map(|(remote, local)| remote == local)
                .unwrap_or(false);

            if !already_current && !seen.contains(&meta.id) {
                let bytes = download_file_bytes(&client, &access_token, &meta.id).await?;
                let snapshot: RemoteSnapshot = decrypt_remote_blob(&bytes, key)?;
                fetch.snapshot = Some((meta.id.clone(), snapshot));
            }
        }

        let missing: Vec<_> = layout
            .batches
            .iter()
            .filter(|file| !seen.contains(&file.id))
            .collect();

        let total = missing.len();
        report_progress(reporter, "downloading", None, 0, total);
        for (index, file) in missing.iter().enumerate() {
            if is_sync_cancelled() {
                return Err(anyhow!("sync_cancelled"));
            }
            let bytes = download_file_bytes(&client, &access_token, &file.id).await?;
            match decrypt_remote_blob::<MutationBatch>(&bytes, key) {
                Ok(batch) => fetch.batches.push((file.id.clone(), batch)),
                // Um lote ilegível pertence a outra chave mestre ou está
                // corrompido. Ignorar é melhor que travar a sincronia inteira:
                // os demais lotes continuam válidos.
                Err(_) => fetch.unreadable += 1,
            }
            report_progress(reporter, "downloading", None, index + 1, total);
        }

        report_progress(reporter, "complete", None, total, total);
        Ok(fetch)
    }

    /// Envia os lotes enfileirados, um arquivo imutável por lote.
    ///
    /// Devolve, para cada lote, o id do arquivo criado — é o que o cofre grava
    /// para não baixar de volta o que ele mesmo acabou de publicar.
    pub async fn push_batches(
        &mut self,
        reporter: &Reporter,
        key: &[u8; 32],
        header: &RemoteHeader,
        batches: &[MutationBatch],
    ) -> Result<Vec<(Uuid, String)>> {
        clear_sync_cancel();
        let client = Client::new();
        let access_token = self.access_token().await?;

        let folder_id = ensure_vault_folder(&client, &access_token, &vault_scope(), true)
            .await?
            .ok_or_else(|| anyhow!("Falha ao preparar pasta OpenPtl no Google Drive"))?;

        let layout =
            RemoteLayout::classify(list_drive_files(&client, &access_token, &folder_id).await?);
        ensure_remote_header(&client, &access_token, &folder_id, &layout, header).await?;

        let total = batches.len();
        report_progress(reporter, "uploading", None, 0, total);

        let mut pushed = Vec::new();
        for (index, batch) in batches.iter().enumerate() {
            if is_sync_cancelled() {
                return Err(anyhow!("sync_cancelled"));
            }
            let bytes = encrypt_remote_blob(batch, key)?;
            let created = create_drive_object(
                &client,
                &access_token,
                &folder_id,
                &batch.file_name(),
                bytes,
            )
            .await?;
            pushed.push((batch.mutation_id, created.id));
            report_progress(reporter, "uploading", None, index + 1, total);
        }

        report_progress(reporter, "complete", None, total, total);
        Ok(pushed)
    }

    /// Publica um snapshot e apaga os lotes que ele resume.
    ///
    /// Podar é seguro porque o snapshot é estado completo: um aparelho que
    /// ficou para trás adota o snapshot e reenvia o que ainda tinha na fila,
    /// que o relógio lógico reconcilia.
    pub async fn compact(
        &mut self,
        reporter: &Reporter,
        key: &[u8; 32],
        snapshot: &RemoteSnapshot,
    ) -> Result<String> {
        let client = Client::new();
        let access_token = self.access_token().await?;

        let folder_id = ensure_vault_folder(&client, &access_token, &vault_scope(), true)
            .await?
            .ok_or_else(|| anyhow!("Falha ao preparar pasta OpenPtl no Google Drive"))?;

        let layout =
            RemoteLayout::classify(list_drive_files(&client, &access_token, &folder_id).await?);

        let bytes = encrypt_remote_blob(snapshot, key)?;
        let created = create_drive_object(
            &client,
            &access_token,
            &folder_id,
            &snapshot.file_name(),
            bytes,
        )
        .await?;

        let covered: HashSet<Uuid> = snapshot.covered.iter().copied().collect();
        let removable: Vec<_> = layout
            .batches
            .iter()
            .filter(|file| {
                batch_id_from_name(file.file_name())
                    .map(|id| covered.contains(&id))
                    .unwrap_or(false)
            })
            .chain(layout.stale_snapshots().iter())
            .collect();

        let total = removable.len();
        report_progress(reporter, "compacting", None, 0, total);
        for (index, file) in removable.iter().enumerate() {
            delete_drive_file(&client, &access_token, &file.id).await?;
            report_progress(reporter, "compacting", None, index + 1, total);
        }
        report_progress(reporter, "complete", None, total, total);

        Ok(created.id)
    }

    /// Diz se há cofre no Drive e devolve o cabeçalho, que carrega o salt
    /// necessário para validar a senha mestre num aparelho novo.
    pub async fn probe_remote(&mut self) -> Result<Option<RemoteHeader>> {
        let client = Client::new();
        let access_token = self.access_token().await?;

        let Some(folder_id) =
            ensure_vault_folder(&client, &access_token, &vault_scope(), false).await?
        else {
            return Ok(None);
        };

        let layout =
            RemoteLayout::classify(list_drive_files(&client, &access_token, &folder_id).await?);
        let Some(file) = layout.header.as_ref() else {
            return Ok(None);
        };

        read_remote_header(&client, &access_token, file)
            .await
            .map(Some)
    }

    /// Endereços do servidor de auth, guardados para as chamadas seguintes.
    pub fn use_servers(&mut self, address: String, fallbacks: Vec<String>) {
        set_auth_endpoints(address, fallbacks);
    }

    /// Cofre sobre o qual as próximas operações agem.
    pub fn use_vault(&mut self, vault_id: String) {
        set_vault_scope(vault_id);
    }

    async fn access_token(&self) -> Result<String> {
        let (address, fallbacks) = auth_endpoints();
        access_token_from_refresh_with_fallback(&address, &fallbacks).await
    }
}

/// O que uma leitura do Drive trouxe.
#[derive(Debug, Default)]
pub struct RemoteFetch {
    pub snapshot: Option<(String, RemoteSnapshot)>,
    pub batches: Vec<(String, MutationBatch)>,
    /// Quantos lotes existem lá, para decidir se é hora de compactar.
    pub remote_batch_count: usize,
    /// Lotes que não abriram com a chave atual.
    pub unreadable: usize,
}

fn batch_id_from_name(name: &str) -> Option<Uuid> {
    Uuid::parse_str(name.strip_suffix(".bin")?).ok()
}

fn snapshot_id_from_name(name: &str) -> Option<Uuid> {
    let trimmed = name
        .strip_prefix(REMOTE_SNAPSHOT_PREFIX)?
        .strip_suffix(".bin")?;
    Uuid::parse_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::REMOTE_HEADER_FILE_NAME;

    #[test]
    fn a_batch_file_name_round_trips_through_its_id() {
        let id = Uuid::now_v7();
        assert_eq!(batch_id_from_name(&format!("{id}.bin")), Some(id));
    }

    #[test]
    fn a_snapshot_is_never_mistaken_for_a_batch() {
        let id = Uuid::now_v7();
        let name = format!("{REMOTE_SNAPSHOT_PREFIX}{id}.bin");
        assert_eq!(batch_id_from_name(&name), None);
        assert_eq!(snapshot_id_from_name(&name), Some(id));
    }

    #[test]
    fn the_header_is_neither_a_batch_nor_a_snapshot() {
        assert_eq!(batch_id_from_name(REMOTE_HEADER_FILE_NAME), None);
        assert_eq!(snapshot_id_from_name(REMOTE_HEADER_FILE_NAME), None);
    }
}
