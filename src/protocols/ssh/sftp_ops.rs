use super::*;

impl SshManager {
    pub async fn sftp_list(&mut self, session_id: &str, path: &str) -> Result<Vec<SftpEntry>> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;

        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;
        let read_dir = sftp
            .read_dir(target.clone())
            .await
            .with_context(|| format!("Falha ao listar diretorio remoto: {}", target))?;

        let mut mapped = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            let metadata = entry.metadata();
            mapped.push(SftpEntry {
                name: name.clone(),
                path: join_remote_path(&target, &name),
                is_dir: metadata.is_dir(),
                size: metadata.size.unwrap_or_default(),
                permissions: metadata.permissions,
                modified_at: metadata.mtime.map(|value| value as i64),
            });
        }

        Ok(mapped)
    }

    pub async fn sftp_read(
        &mut self,
        session_id: &str,
        path: &str,
        chunk_size: usize,
    ) -> Result<String> {
        let bytes = self.sftp_read_bytes(session_id, path, chunk_size).await?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    pub async fn sftp_read_bytes(
        &mut self,
        session_id: &str,
        path: &str,
        chunk_size: usize,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.sftp_download_to_writer(session_id, path, &mut bytes, chunk_size, |_| {})
            .await?;

        Ok(bytes)
    }

    pub async fn sftp_read_bytes_with_limit(
        &mut self,
        session_id: &str,
        path: &str,
        chunk_size: usize,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;

        let mut file = sftp
            .open(target.clone())
            .await
            .with_context(|| format!("Falha ao abrir arquivo remoto: {}", target))?;

        let mut bytes = Vec::new();
        let mut total = 0u64;
        let mut buffer = vec![0u8; normalize_chunk_size(chunk_size)];

        loop {
            let size = file
                .read(&mut buffer)
                .await
                .with_context(|| format!("Falha ao ler arquivo remoto: {}", target))?;
            if size == 0 {
                break;
            }

            total = total.saturating_add(size as u64);
            if total > max_bytes {
                return Ok(None);
            }

            bytes.extend_from_slice(&buffer[..size]);
        }

        Ok(Some(bytes))
    }

    pub async fn sftp_read_chunk(
        &mut self,
        session_id: &str,
        path: &str,
        offset: u64,
        chunk_size: usize,
    ) -> Result<(Vec<u8>, u64, bool)> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;

        let total = sftp
            .metadata(target.clone())
            .await
            .ok()
            .and_then(|metadata| metadata.size)
            .unwrap_or(0);

        let mut file = sftp
            .open(target.clone())
            .await
            .with_context(|| format!("Falha ao abrir arquivo remoto: {}", target))?;

        file.seek(SeekFrom::Start(offset))
            .await
            .with_context(|| format!("Falha ao posicionar leitura remota em {}", target))?;

        let mut buffer = vec![0u8; normalize_chunk_size(chunk_size)];
        let size = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("Falha ao ler chunk remoto: {}", target))?;
        buffer.truncate(size);

        let bytes_read = offset.saturating_add(size as u64);
        let eof = size == 0 || bytes_read >= total;

        Ok((buffer, total, eof))
    }

    pub async fn sftp_write(
        &mut self,
        session_id: &str,
        path: &str,
        content: &str,
        chunk_size: usize,
    ) -> Result<()> {
        self.sftp_write_bytes(session_id, path, content.as_bytes(), chunk_size)
            .await
    }

    pub async fn sftp_rename(
        &mut self,
        session_id: &str,
        from_path: &str,
        to_path: &str,
    ) -> Result<()> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let from = normalize_remote_path(from_path);
        let to = normalize_remote_path(to_path);
        let sftp = ensure_sftp_session(managed).await?;
        sftp.rename(from.clone(), to.clone())
            .await
            .with_context(|| format!("Falha ao renomear item remoto de {} para {}", from, to))?;
        Ok(())
    }

    pub async fn sftp_delete(&mut self, session_id: &str, path: &str, is_dir: bool) -> Result<()> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;

        if is_dir {
            sftp.remove_dir(target.clone())
                .await
                .with_context(|| format!("Falha ao remover pasta remota: {}", target))?;
        } else {
            sftp.remove_file(target.clone())
                .await
                .with_context(|| format!("Falha ao remover arquivo remoto: {}", target))?;
        }
        Ok(())
    }

    pub async fn sftp_mkdir(&mut self, session_id: &str, path: &str) -> Result<()> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;
        sftp.create_dir(target.clone())
            .await
            .with_context(|| format!("Falha ao criar pasta remota: {}", target))?;
        Ok(())
    }

    pub async fn sftp_create_file(&mut self, session_id: &str, path: &str) -> Result<()> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;
        let mut file = sftp
            .create(target.clone())
            .await
            .with_context(|| format!("Falha ao criar arquivo remoto: {}", target))?;
        let _ = file.shutdown().await;
        Ok(())
    }

    pub async fn sftp_write_bytes(
        &mut self,
        session_id: &str,
        path: &str,
        content: &[u8],
        chunk_size: usize,
    ) -> Result<()> {
        let mut cursor = std::io::Cursor::new(content);
        self.sftp_upload_from_reader(session_id, path, &mut cursor, chunk_size, |_| {})
            .await?;
        Ok(())
    }

    pub async fn sftp_file_size(&mut self, session_id: &str, path: &str) -> Result<Option<u64>> {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;

        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;

        match sftp.metadata(target).await {
            Ok(metadata) => Ok(metadata.size),
            Err(_) => Ok(None),
        }
    }

    pub async fn sftp_download_to_writer<W, F>(
        &mut self,
        session_id: &str,
        path: &str,
        writer: &mut W,
        chunk_size: usize,
        mut on_chunk: F,
    ) -> Result<u64>
    where
        W: Write,
        F: FnMut(u64),
    {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;

        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;

        let mut file = sftp
            .open(target.clone())
            .await
            .with_context(|| format!("Falha ao abrir arquivo remoto: {}", target))?;

        let mut transferred = 0u64;
        let mut buffer = vec![0u8; normalize_chunk_size(chunk_size)];

        loop {
            let size = file
                .read(&mut buffer)
                .await
                .with_context(|| format!("Falha ao ler arquivo remoto: {}", target))?;
            if size == 0 {
                break;
            }

            writer
                .write_all(&buffer[..size])
                .with_context(|| format!("Falha ao escrever chunk recebido de {}", target))?;

            transferred = transferred.saturating_add(size as u64);
            on_chunk(size as u64);
        }

        Ok(transferred)
    }

    pub async fn sftp_upload_from_reader<R, F>(
        &mut self,
        session_id: &str,
        path: &str,
        reader: &mut R,
        chunk_size: usize,
        mut on_chunk: F,
    ) -> Result<u64>
    where
        R: Read,
        F: FnMut(u64),
    {
        let managed = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;

        let target = normalize_remote_path(path);
        let sftp = ensure_sftp_session(managed).await?;
        let mut file = sftp
            .create(target.clone())
            .await
            .with_context(|| format!("Falha ao criar arquivo remoto: {}", target))?;

        let mut transferred = 0u64;
        let mut buffer = vec![0u8; normalize_chunk_size(chunk_size)];
        loop {
            let size = reader
                .read(&mut buffer)
                .with_context(|| format!("Falha ao ler origem para upload: {}", target))?;
            if size == 0 {
                break;
            }

            file.write_all(&buffer[..size])
                .await
                .with_context(|| format!("Falha ao escrever arquivo remoto: {}", target))?;

            transferred = transferred.saturating_add(size as u64);
            on_chunk(size as u64);
        }

        file.shutdown()
            .await
            .with_context(|| format!("Falha ao finalizar arquivo remoto: {}", target))?;

        Ok(transferred)
    }

    pub fn sessions_share_profile(&self, left_session_id: &str, right_session_id: &str) -> bool {
        let left = self.sessions.get(left_session_id);
        let right = self.sessions.get(right_session_id);
        match (left, right) {
            (Some(left_session), Some(right_session)) => {
                left_session.info.profile_id == right_session.info.profile_id
            }
            _ => false,
        }
    }

    pub async fn sftp_copy_between_sessions(
        &mut self,
        from_session_id: &str,
        to_session_id: &str,
        from_path: &str,
        to_path: &str,
    ) -> Result<()> {
        if from_session_id != to_session_id
            && !self.sessions_share_profile(from_session_id, to_session_id)
        {
            return Err(anyhow!(
                "Copia remota otimizada requer sessoes no mesmo perfil"
            ));
        }

        let managed = self
            .sessions
            .get_mut(from_session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", from_session_id))?;

        let source = normalize_remote_path(from_path);
        let target = normalize_remote_path(to_path);
        if source == target {
            return Ok(());
        }

        let source_is_dir = {
            let sftp = ensure_sftp_session(managed).await?;
            let stat = sftp
                .metadata(source.clone())
                .await
                .with_context(|| format!("Falha ao obter metadata remota: {}", source))?;
            stat.is_dir()
        };

        run_remote_copy_command(&managed.handle, &source, &target, source_is_dir).await
    }
}
