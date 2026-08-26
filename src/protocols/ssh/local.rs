use super::*;

pub(super) fn spawn_local_shell(
    start_path: Option<&Path>,
) -> Result<(Child, ChildStdin, ChildStdout, ChildStderr)> {
    #[cfg(target_os = "windows")]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("powershell");
        cmd.arg("-NoLogo").arg("-NoProfile");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut cmd = Command::new("bash");
        cmd.arg("-i");
        cmd
    };

    if let Some(path) = start_path {
        command.current_dir(path);
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start local terminal process")?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Failed to capture local terminal stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture local terminal stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture local terminal stderr"))?;

    Ok((child, stdin, stdout, stderr))
}

pub(super) fn pump_reader_into_buffer<R>(mut reader: R, output: Arc<Mutex<Vec<u8>>>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if let Ok(mut guard) = output.lock() {
                        guard.extend_from_slice(&buffer[..size]);
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

pub(super) fn drain_local_output(output: &Arc<Mutex<Vec<u8>>>) -> String {
    if let Ok(mut guard) = output.lock() {
        if guard.is_empty() {
            return String::new();
        }
        let bytes = guard.drain(..).collect::<Vec<_>>();
        return String::from_utf8_lossy(&bytes).to_string();
    }
    String::new()
}

pub(super) async fn auth_with_private_key(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> Result<bool> {
    let key = keys::decode_secret_key(private_key, passphrase)
        .context("Falha ao carregar chave privada SSH")?;

    let hash_alg = if key.algorithm().is_rsa() {
        handle
            .best_supported_rsa_hash()
            .await
            .context("Falha ao negociar algoritmo RSA com servidor SSH")?
            .flatten()
    } else {
        None
    };

    let auth = handle
        .authenticate_publickey(
            username,
            PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
        )
        .await
        .context("Falha ao autenticar com chave privada")?;

    Ok(auth.success())
}

pub(super) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        "/".to_string()
    } else if trimmed.starts_with('/') {
        trimmed
    } else {
        format!("/{}", trimmed)
    }
}

pub(super) fn join_remote_path(base: &str, child: &str) -> String {
    let base = normalize_remote_path(base);
    let child = child.trim().trim_start_matches('/');
    if base == "/" {
        format!("/{}", child)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), child)
    }
}

pub(super) fn normalize_chunk_size(chunk_size: usize) -> usize {
    chunk_size.clamp(64 * 1024, 8 * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::update_mouse_sgr_mode;

    #[test]
    fn should_toggle_sgr_mouse_mode_from_terminal_output() {
        let mut enabled = false;
        update_mouse_sgr_mode("\x1b[?1006h", &mut enabled);
        assert!(enabled);

        update_mouse_sgr_mode("\x1b[?1006l", &mut enabled);
        assert!(!enabled);
    }

    #[test]
    fn should_ignore_non_sgr_mouse_sequences() {
        let mut enabled = false;
        update_mouse_sgr_mode("\x1b[?1000h", &mut enabled);
        assert!(!enabled);

        enabled = true;
        update_mouse_sgr_mode("\x1b[?25l", &mut enabled);
        assert!(enabled);
    }
}
