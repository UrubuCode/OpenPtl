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

        let Some(folder_id) = ensure_openptl_folder(&client, &access_token, false).await? else {
            return Ok(());
        };

        let remote_files = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        for (_, metadata) in remote_files {
            delete_drive_file(&client, &access_token, &metadata.id).await?;
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

    pub async fn push(
        &mut self,
        reporter: &Reporter,
        local_files: Vec<(String, Vec<u8>)>,
        server_address: &str,
        fallback_addresses: &[String],
    ) -> Result<SyncMetadata> {
        clear_sync_cancel();
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        if is_sync_cancelled() {
            return Err(anyhow!("sync_cancelled"));
        }

        let client = Client::new();
        let folder_id = ensure_openptl_folder(&client, &access_token, true)
            .await?
            .ok_or_else(|| anyhow!("Falha ao preparar pasta OpenPtl no Google Drive"))?;
        let remote_files = list_drive_bin_files(&client, &access_token, &folder_id).await?;

        let mut local_names = HashSet::new();
        for (name, _) in &local_files {
            local_names.insert(name.clone());
        }
        let stale_remote_count = remote_files
            .keys()
            .filter(|name| !local_names.contains(*name))
            .count();
        let total_steps = local_files.len() + stale_remote_count;
        let mut processed_steps = 0usize;
        report_progress(reporter, "uploading", None, processed_steps, total_steps);

        for (name, bytes) in local_files {
            if let Some(existing) = remote_files.get(&name) {
                upload_file_bytes(&client, &access_token, &existing.id, bytes).await?;
            } else {
                let created = create_drive_file(&client, &access_token, &folder_id, &name).await?;
                upload_file_bytes(&client, &access_token, &created.id, bytes).await?;
            }
            processed_steps = processed_steps.saturating_add(1);
            report_progress(
                reporter,
                "uploading",
                Some(name.as_str()),
                processed_steps,
                total_steps,
            );
        }

        for (name, file_meta) in remote_files {
            if !local_names.contains(&name) {
                delete_drive_file(&client, &access_token, &file_meta.id).await?;
                processed_steps = processed_steps.saturating_add(1);
                report_progress(
                    reporter,
                    "cleaning_remote",
                    Some(name.as_str()),
                    processed_steps,
                    total_steps,
                );
            }
        }

        report_progress(reporter, "complete", None, total_steps, total_steps);

        let now = Utc::now();
        Ok(SyncMetadata {
            last_remote_modified: Some(now.to_rfc3339()),
            last_sync_at: Some(now.to_rfc3339()),
            last_local_change: now.timestamp(),
        })
    }

    pub async fn pull(
        &mut self,
        reporter: &Reporter,
        server_address: &str,
        fallback_addresses: &[String],
    ) -> Result<Option<HashMap<String, Vec<u8>>>> {
        clear_sync_cancel();
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        if is_sync_cancelled() {
            return Err(anyhow!("sync_cancelled"));
        }

        let client = Client::new();
        let Some(folder_id) = ensure_openptl_folder(&client, &access_token, false).await? else {
            return Ok(None);
        };

        let remote_files = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        if !remote_files.contains_key(OPENPTL_FILE_NAME)
            || !remote_files.contains_key(PROFILE_FILE_NAME)
            || !remote_files.contains_key(MANIFEST_FILE_NAME)
        {
            return Ok(None);
        }

        let total_steps = remote_files.len();
        let mut processed_steps = 0usize;
        report_progress(reporter, "downloading", None, processed_steps, total_steps);
        let mut snapshot = HashMap::new();
        for (name, file_meta) in &remote_files {
            let bytes = download_file_bytes(&client, &access_token, &file_meta.id).await?;
            snapshot.insert(name.clone(), bytes);
            processed_steps = processed_steps.saturating_add(1);
            report_progress(
                reporter,
                "downloading",
                Some(name.as_str()),
                processed_steps,
                total_steps,
            );
        }

        report_progress(reporter, "complete", None, total_steps, total_steps);
        Ok(Some(snapshot))
    }

    pub async fn startup_conflicts(
        &mut self,
        vault: &VaultManager,
        server_address: &str,
        fallback_addresses: &[String],
    ) -> Result<SyncConflictPreview> {
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        let client = Client::new();

        let Some(folder_id) = ensure_openptl_folder(&client, &access_token, false).await? else {
            return Ok(SyncConflictPreview::default());
        };

        let remote_files = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        let Some(remote_manifest_meta) = remote_files.get(MANIFEST_FILE_NAME) else {
            return Ok(SyncConflictPreview::default());
        };

        let remote_manifest_bytes =
            download_file_bytes(&client, &access_token, &remote_manifest_meta.id).await?;

        let remote_manifest = vault.decrypt_manifest_bytes(&remote_manifest_bytes)?;
        let local_manifest = vault.local_manifest_snapshot()?;

        let mut conflicts = Vec::new();

        let mut host_ids = HashSet::new();
        host_ids.extend(local_manifest.hosts.keys().cloned());
        host_ids.extend(remote_manifest.hosts.keys().cloned());
        for id in host_ids {
            let local_hash = local_manifest.hosts.get(&id).cloned();
            let remote_hash = remote_manifest.hosts.get(&id).cloned();
            if local_hash != remote_hash {
                conflicts.push(SyncConflictItem {
                    kind: SyncConflictKind::Host,
                    id: id.clone(),
                    label: format!("Host {}", id),
                    local_hash,
                    remote_hash,
                });
            }
        }

        let mut keychain_ids = HashSet::new();
        keychain_ids.extend(local_manifest.keychain.keys().cloned());
        keychain_ids.extend(remote_manifest.keychain.keys().cloned());
        for id in keychain_ids {
            let local_hash = local_manifest.keychain.get(&id).cloned();
            let remote_hash = remote_manifest.keychain.get(&id).cloned();
            if local_hash != remote_hash {
                conflicts.push(SyncConflictItem {
                    kind: SyncConflictKind::Keychain,
                    id: id.clone(),
                    label: format!("Keychain {}", id),
                    local_hash,
                    remote_hash,
                });
            }
        }

        let local_profile_hash = Some(local_manifest.profile.clone());
        let remote_profile_hash = Some(remote_manifest.profile.clone());
        if local_profile_hash != remote_profile_hash {
            conflicts.push(SyncConflictItem {
                kind: SyncConflictKind::Profile,
                id: "profile".to_string(),
                label: "Profile / Settings".to_string(),
                local_hash: local_profile_hash,
                remote_hash: remote_profile_hash,
            });
        }

        conflicts.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(SyncConflictPreview { conflicts })
    }

    pub async fn resolve_startup_conflicts(
        &mut self,
        reporter: &Reporter,
        vault: &mut VaultManager,
        server_address: &str,
        fallback_addresses: &[String],
        decisions: Vec<SyncConflictDecision>,
    ) -> Result<SyncState> {
        clear_sync_cancel();
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        let client = Client::new();
        let folder_id = ensure_openptl_folder(&client, &access_token, true)
            .await?
            .ok_or_else(|| anyhow!("Falha ao preparar pasta OpenPtl no Google Drive"))?;

        let remote_files = list_drive_bin_files(&client, &access_token, &folder_id).await?;

        let mut client_overrides: HashMap<String, Option<Vec<u8>>> = HashMap::new();
        for decision in decisions {
            if decision.keep != SyncKeepSide::Client {
                continue;
            }
            let file_name = conflict_file_name(&decision)?;
            let bytes = vault.read_local_bin_file(&file_name)?;
            client_overrides.insert(file_name, bytes);
        }

        let mut remote_snapshot = HashMap::new();
        for (name, metadata) in &remote_files {
            let bytes = download_file_bytes(&client, &access_token, &metadata.id).await?;
            remote_snapshot.insert(name.clone(), bytes);
        }

        if !remote_snapshot.is_empty() {
            vault.replace_local_files(&remote_snapshot)?;
        }

        for (name, maybe_bytes) in client_overrides {
            if let Some(bytes) = maybe_bytes {
                vault.write_local_bin_file(&name, &bytes)?;
            } else {
                vault.remove_local_bin_file(&name)?;
            }
        }

        vault.reload_unlocked_from_disk_and_persist()?;

        let local_files = vault.list_local_bin_files()?;
        let latest_remote = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        let mut local_names = HashSet::new();

        for (name, bytes) in local_files {
            local_names.insert(name.clone());
            if let Some(existing) = latest_remote.get(&name) {
                upload_file_bytes(&client, &access_token, &existing.id, bytes).await?;
            } else {
                let created = create_drive_file(&client, &access_token, &folder_id, &name).await?;
                upload_file_bytes(&client, &access_token, &created.id, bytes).await?;
            }
        }

        for (name, metadata) in latest_remote {
            if !local_names.contains(&name) {
                delete_drive_file(&client, &access_token, &metadata.id).await?;
            }
        }

        let now = Utc::now();
        let mut next_metadata = vault.sync_metadata()?;
        next_metadata.last_sync_at = Some(now.to_rfc3339());
        next_metadata.last_remote_modified = Some(now.to_rfc3339());
        next_metadata.last_local_change = now.timestamp();
        vault.set_sync_metadata(next_metadata.clone())?;

        let state = SyncState::ok("sync_conflicts_resolved", next_metadata.last_sync_at);
        reporter.status(state.clone());
        Ok(state)
    }

    pub async fn recovery_probe(
        &mut self,
        server_address: &str,
        fallback_addresses: &[String],
    ) -> Result<RecoveryProbeResult> {
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        let client = Client::new();

        let Some(folder_id) = ensure_openptl_folder(&client, &access_token, false).await? else {
            return Ok(RecoveryProbeResult {
                found: false,
                message: BackendMessage::key("sync_probe_folder_not_found"),
            });
        };

        let files = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        if files.contains_key(OPENPTL_FILE_NAME) {
            Ok(RecoveryProbeResult {
                found: true,
                message: BackendMessage::key("sync_probe_backup_found"),
            })
        } else {
            Ok(RecoveryProbeResult {
                found: false,
                message: BackendMessage::key("sync_probe_backup_not_found"),
            })
        }
    }

    pub async fn recovery_restore(
        &mut self,
        reporter: &Reporter,
        vault: &mut VaultManager,
        server_address: &str,
        fallback_addresses: &[String],
        password: String,
    ) -> Result<VaultStatus> {
        clear_sync_cancel();
        let access_token =
            access_token_from_refresh_with_fallback(server_address, fallback_addresses).await?;
        let client = Client::new();

        let Some(folder_id) = ensure_openptl_folder(&client, &access_token, false).await? else {
            return Err(anyhow!("sync_recovery_folder_not_found"));
        };

        let files = list_drive_bin_files(&client, &access_token, &folder_id).await?;
        let openptl_meta = files
            .get(OPENPTL_FILE_NAME)
            .ok_or_else(|| anyhow!("sync_recovery_openptl_not_found"))?;
        let openptl_bytes = download_file_bytes(&client, &access_token, &openptl_meta.id).await?;

        if !vault.validate_password_for_openptl_bytes(&openptl_bytes, password.trim())? {
            return Err(anyhow!("sync_recovery_invalid_master_password"));
        }

        let pending = SyncState {
            connected: true,
            status: "running".to_string(),
            message: BackendMessage::key("sync_recovery_downloading"),
            last_sync_at: None,
            pending_user_code: None,
            verification_url: None,
        };
        reporter.status(pending.clone());

        let mut snapshot = HashMap::new();
        for (name, metadata) in files {
            let bytes = download_file_bytes(&client, &access_token, &metadata.id).await?;
            snapshot.insert(name, bytes);
        }

        if !snapshot.contains_key(OPENPTL_FILE_NAME)
            || !snapshot.contains_key(PROFILE_FILE_NAME)
            || !snapshot.contains_key(MANIFEST_FILE_NAME)
        {
            return Err(anyhow!("sync_backup_incomplete"));
        }

        vault.replace_local_files(&snapshot)?;
        let status = vault.unlock(Some(password))?;
        Ok(status)
    }
}

fn conflict_file_name(decision: &SyncConflictDecision) -> Result<String> {
    match decision.kind {
        SyncConflictKind::Profile => Ok(PROFILE_FILE_NAME.to_string()),
        SyncConflictKind::Host | SyncConflictKind::Keychain => {
            if uuid::Uuid::parse_str(decision.id.trim()).is_err() {
                return Err(anyhow!("sync_conflict_invalid_id"));
            }
            Ok(format!("{}.bin", decision.id.trim()))
        }
    }
}
