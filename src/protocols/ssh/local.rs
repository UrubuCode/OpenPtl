use super::*;

pub(crate) fn spawn_local_shell(
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

pub(crate) fn pump_reader_into_buffer<R>(mut reader: R, output: Arc<Mutex<Vec<u8>>>)
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

pub(crate) fn drain_local_output(output: &Arc<Mutex<Vec<u8>>>) -> String {
    if let Ok(mut guard) = output.lock() {
        if guard.is_empty() {
            return String::new();
        }
        let bytes = guard.drain(..).collect::<Vec<_>>();
        return String::from_utf8_lossy(&bytes).to_string();
    }
    String::new()
}
