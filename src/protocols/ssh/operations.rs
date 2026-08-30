use super::*;

impl SshManager {
    /// Consome a saida acumulada pela task leitora, sem escrever nada. E o que
    /// um terminal interativo precisa: `run_command` escreve e espera, este
    /// apenas entrega o que chegou desde a ultima chamada.
    pub fn drain_output(&mut self, session_id: &str) -> Result<String> {
        if let Some(managed) = self.sessions.get_mut(session_id) {
            let terminal = managed
                .terminal
                .as_ref()
                .ok_or_else(|| anyhow!("Sessao {} nao suporta shell interativo", session_id))?;
            let output = drain_remote_output(&terminal.output);
            update_mouse_sgr_mode(&output, &mut managed.mouse_sgr_enabled);
            return Ok(output);
        }

        let local = self
            .local_sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Sessao {} nao encontrada", session_id))?;
        Ok(drain_local_output(&local.output))
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
