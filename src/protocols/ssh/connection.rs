use super::*;

impl SshManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            local_sessions: HashMap::new(),
        }
    }

    pub fn list_sessions(&self) -> Vec<SshSessionInfo> {
        let mut sessions = self
            .sessions
            .values()
            .map(|item| item.info.clone())
            .collect::<Vec<_>>();
        sessions.extend(self.local_sessions.values().map(|item| item.info.clone()));
        sessions
    }

    pub async fn connect(
        &mut self,
        profile: &ConnectionProfile,
        known_hosts_path: Option<&Path>,
    ) -> Result<SshSessionInfo> {
        match self
            .connect_ex(
                profile,
                known_hosts_path,
                true,
                SshConnectPurpose::Terminal,
                false,
            )
            .await?
        {
            SshConnectResult::Connected { session } => Ok(session),
            SshConnectResult::UnknownHostChallenge { message, .. } => {
                Err(anyhow!(message.message.clone()))
            }
            SshConnectResult::AuthRequired { message } => Err(anyhow!(message.message.clone())),
            SshConnectResult::Error { message } => Err(anyhow!(message.message.clone())),
        }
    }

    pub async fn connect_ex(
        &mut self,
        profile: &ConnectionProfile,
        known_hosts_path: Option<&Path>,
        accept_unknown_host: bool,
        connect_purpose: SshConnectPurpose,
        _webrtc_enabled: bool,
    ) -> Result<SshConnectResult> {
        let known_hosts_file = resolve_known_hosts_path(
            known_hosts_path
                .map(|path| path.to_string_lossy().to_string())
                .as_deref(),
        )?;
        ensure_known_hosts_file(&known_hosts_file)?;

        let host_key_capture = HostKeyCapture::default();
        let handler = SshClientHandler {
            host_key_capture: host_key_capture.clone(),
        };
        let config = Arc::new(client::Config::default());

        let mut handle = client::connect(config, (profile.host.as_str(), profile.port), handler)
            .await
            .with_context(|| {
                format!(
                    "Handshake SSH falhou ao conectar em {}:{}",
                    profile.host, profile.port
                )
            })?;

        let server_key = host_key_capture
            .get()
            .ok_or_else(|| anyhow!("Servidor SSH nao retornou host key"))?;

        if let Some(challenge) =
            verify_known_host(&server_key, profile, &known_hosts_file, accept_unknown_host)?
        {
            let _ = handle
                .disconnect(Disconnect::ByApplication, "Disconnected", "pt-BR")
                .await;
            return Ok(challenge);
        }

        match authenticate_session(&mut handle, profile).await {
            Ok(()) => {}
            Err(AuthFailure::NeedsInput(message)) => {
                let _ = handle
                    .disconnect(Disconnect::ByApplication, "Disconnected", "pt-BR")
                    .await;
                return Ok(SshConnectResult::AuthRequired { message });
            }
            Err(AuthFailure::Fatal(message)) => {
                let _ = handle
                    .disconnect(Disconnect::ByApplication, "Disconnected", "pt-BR")
                    .await;
                return Ok(SshConnectResult::Error { message });
            }
        }

        let session_id = uuid::Uuid::new_v4().to_string();

        let terminal = if connect_purpose == SshConnectPurpose::Terminal {
            Some(open_terminal_session(&handle).await?)
        } else {
            None
        };

        let info = SshSessionInfo {
            session_id: session_id.clone(),
            profile_id: profile.id.clone(),
            connected_at: Utc::now().timestamp(),
            session_kind: "ssh".to_string(),
        };

        self.sessions.insert(
            session_id,
            ManagedSession {
                info: info.clone(),
                handle,
                terminal,
                sftp: None,
                mouse_sgr_enabled: false,
            },
        );

        Ok(SshConnectResult::Connected { session: info })
    }

    pub fn connect_local(&mut self, start_path: Option<&Path>) -> Result<SshSessionInfo> {
        let (child, stdin, stdout, stderr) = spawn_local_shell(start_path)?;
        let output = Arc::new(Mutex::new(Vec::new()));
        pump_reader_into_buffer(stdout, Arc::clone(&output));
        pump_reader_into_buffer(stderr, Arc::clone(&output));

        let session_id = uuid::Uuid::new_v4().to_string();
        let info = SshSessionInfo {
            session_id: session_id.clone(),
            profile_id: "local".to_string(),
            connected_at: Utc::now().timestamp(),
            session_kind: "local".to_string(),
        };

        let local = LocalManagedSession {
            info: info.clone(),
            child,
            stdin,
            output,
        };
        self.local_sessions.insert(session_id, local);

        Ok(info)
    }

    pub async fn disconnect(&mut self, session_id: &str) {
        if let Some(mut managed) = self.sessions.remove(session_id) {
            if let Some(terminal) = managed.terminal.take() {
                let _ = terminal.writer.eof().await;
                let _ = terminal.writer.close().await;
                terminal.reader_task.abort();
            }

            if let Some(sftp) = managed.sftp.take() {
                let _ = sftp.close().await;
            }

            let _ = managed
                .handle
                .disconnect(Disconnect::ByApplication, "Disconnected", "pt-BR")
                .await;
            return;
        }

        if let Some(mut local) = self.local_sessions.remove(session_id) {
            let _ = local.stdin.flush();
            let _ = local.child.kill();
            let _ = local.child.wait();
        }
    }

    pub async fn run_command(&mut self, session_id: &str, command: &str) -> Result<String> {
        if let Some(managed) = self.sessions.get_mut(session_id) {
            let terminal = managed
                .terminal
                .as_mut()
                .ok_or_else(|| anyhow!("Sessao {} nao suporta shell interativo", session_id))?;

            let payload = command.replace('\r', "\n");
            if !payload.is_empty() {
                write_to_remote_channel(terminal.writer.as_ref(), payload.as_bytes())
                    .await
                    .context("Falha ao enviar entrada para shell SSH")?;
            }

            sleep(Duration::from_millis(140)).await;
            let output = drain_remote_output(&terminal.output);
            update_mouse_sgr_mode(&output, &mut managed.mouse_sgr_enabled);
            return Ok(output);
        }

        let local = self
            .local_sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;

        let payload = command.replace('\r', "\n");
        if !payload.is_empty() {
            local
                .stdin
                .write_all(payload.as_bytes())
                .context("Falha ao enviar entrada para terminal local")?;
            local
                .stdin
                .flush()
                .context("Falha ao flush do terminal local")?;
        }

        thread::sleep(Duration::from_millis(80));
        Ok(drain_local_output(&local.output))
    }

    pub async fn write_raw_input(&mut self, session_id: &str, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }

        if let Some(managed) = self.sessions.get_mut(session_id) {
            let terminal = managed
                .terminal
                .as_mut()
                .ok_or_else(|| anyhow!("Sessao {} nao suporta shell interativo", session_id))?;
            write_to_remote_channel(terminal.writer.as_ref(), bytes)
                .await
                .context("Falha ao enviar input bruto para shell SSH")?;
            return Ok(());
        }

        if let Some(local) = self.local_sessions.get_mut(session_id) {
            local
                .stdin
                .write_all(bytes)
                .context("Falha ao enviar input bruto para terminal local")?;
            local
                .stdin
                .flush()
                .context("Falha ao flush de input bruto local")?;
            return Ok(());
        }

        Err(anyhow!("Sessao {} nao encontrada", session_id))
    }

    pub fn is_mouse_sgr_enabled(&self, session_id: &str) -> Result<bool> {
        if let Some(managed) = self.sessions.get(session_id) {
            return Ok(managed.mouse_sgr_enabled);
        }
        if self.local_sessions.contains_key(session_id) {
            return Ok(false);
        }
        Err(anyhow!("Sessao {} nao encontrada", session_id))
    }

    pub async fn resize_pty(&mut self, session_id: &str, cols: u32, rows: u32) -> Result<()> {
        if let Some(managed) = self.sessions.get_mut(session_id) {
            let terminal = managed.terminal.as_mut().ok_or_else(|| {
                anyhow!("Sessao {} nao suporta redimensionamento PTY", session_id)
            })?;
            terminal
                .writer
                .window_change(cols, rows, 0, 0)
                .await
                .context("Falha ao redimensionar PTY SSH")?;
            return Ok(());
        }

        if self.local_sessions.contains_key(session_id) {
            return Ok(());
        }

        Err(anyhow!("Sessao {} nao encontrada", session_id))
    }
}
