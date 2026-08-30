use super::*;

pub(crate) async fn ensure_sftp_session(managed: &mut ManagedSession) -> Result<&mut SftpSession> {
    if managed.sftp.is_none() {
        let channel = managed
            .handle
            .channel_open_session()
            .await
            .context("Falha ao abrir canal SFTP")?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .context("Falha ao iniciar subsistema SFTP")?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .context("Falha ao inicializar sessao SFTP")?;
        managed.sftp = Some(sftp);
    }

    managed
        .sftp
        .as_mut()
        .ok_or_else(|| anyhow!("Falha ao inicializar sessao SFTP"))
}

pub(crate) async fn open_terminal_session(
    handle: &client::Handle<SshClientHandler>,
) -> Result<TerminalSession> {
    let channel = handle
        .channel_open_session()
        .await
        .context("Falha ao abrir canal shell SSH")?;
    channel
        .request_pty(true, "xterm", 160, 48, 0, 0, &[])
        .await
        .context("Falha ao solicitar PTY SSH")?;
    channel
        .request_shell(true)
        .await
        .context("Falha ao iniciar shell SSH")?;

    let (read_half, write_half) = channel.split();
    let writer = Arc::new(write_half);
    let output = Arc::new(Mutex::new(Vec::new()));

    let reader_task = spawn_terminal_reader(read_half, Arc::clone(&output));

    Ok(TerminalSession {
        writer,
        output,
        reader_task,
    })
}

pub(crate) fn spawn_terminal_reader(
    mut read_half: ChannelReadHalf,
    output: Arc<Mutex<Vec<u8>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(message) = read_half.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    if let Ok(mut guard) = output.lock() {
                        guard.extend_from_slice(data.as_ref());
                    } else {
                        break;
                    }
                }
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
    })
}

pub(crate) fn drain_remote_output(output: &Arc<Mutex<Vec<u8>>>) -> String {
    if let Ok(mut guard) = output.lock() {
        if guard.is_empty() {
            return String::new();
        }
        let bytes = guard.drain(..).collect::<Vec<_>>();
        return String::from_utf8_lossy(&bytes).to_string();
    }
    String::new()
}

pub(crate) async fn write_to_remote_channel(
    writer: &ChannelWriteHalf<client::Msg>,
    bytes: &[u8],
) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes.to_vec());
    writer
        .data(cursor)
        .await
        .context("Falha ao escrever no canal SSH")
}

pub(crate) async fn run_remote_copy_command(
    handle: &client::Handle<SshClientHandler>,
    source: &str,
    target: &str,
    source_is_dir: bool,
) -> Result<()> {
    let source_quoted = shell_quote_posix(source);
    let target_quoted = shell_quote_posix(target);

    let cp_a = format!("cp -a -- {} {}", source_quoted, target_quoted);
    if run_remote_exec(handle, cp_a.as_str()).await.is_ok() {
        return Ok(());
    }

    let recursive_flag = if source_is_dir { "-R" } else { "" };
    let cp_r = if recursive_flag.is_empty() {
        format!("cp -- {} {}", source_quoted, target_quoted)
    } else {
        format!(
            "cp {} -- {} {}",
            recursive_flag, source_quoted, target_quoted
        )
    };
    run_remote_exec(handle, cp_r.as_str()).await
}

pub(crate) async fn run_remote_exec(
    handle: &client::Handle<SshClientHandler>,
    command: &str,
) -> Result<()> {
    let mut channel = handle
        .channel_open_session()
        .await
        .context("Falha ao abrir canal exec SSH")?;
    channel
        .exec(true, command)
        .await
        .with_context(|| format!("Falha ao executar comando remoto: {}", command))?;

    let mut output = Vec::new();
    let mut exit_status = None::<u32>;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                output.extend_from_slice(data.as_ref());
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                exit_status = Some(status);
            }
            ChannelMsg::Eof => {}
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    let status = exit_status.unwrap_or(255);
    if status == 0 {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output).trim().to_string();
    if stderr.is_empty() {
        return Err(anyhow!(
            "Comando remoto retornou status {} sem detalhes",
            status
        ));
    }

    Err(anyhow!(
        "Comando remoto retornou status {}: {}",
        status,
        stderr
    ))
}

pub(crate) fn shell_quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
