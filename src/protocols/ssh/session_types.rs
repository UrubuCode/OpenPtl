use super::*;

pub(super) struct ManagedSession {
    pub(super) info: SshSessionInfo,
    pub(super) handle: client::Handle<SshClientHandler>,
    pub(super) terminal: Option<TerminalSession>,
    pub(super) sftp: Option<SftpSession>,
    pub(super) mouse_sgr_enabled: bool,
}

pub(super) struct TerminalSession {
    pub(super) writer: Arc<ChannelWriteHalf<client::Msg>>,
    pub(super) output: Arc<Mutex<Vec<u8>>>,
    pub(super) reader_task: JoinHandle<()>,
}

pub(super) struct LocalManagedSession {
    pub(super) info: SshSessionInfo,
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) output: Arc<Mutex<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub(super) struct HostKeyCapture {
    inner: Arc<Mutex<Option<keys::PublicKey>>>,
}

impl HostKeyCapture {
    pub(super) fn set(&self, key: &keys::PublicKey) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(key.clone());
        }
    }

    pub(super) fn get(&self) -> Option<keys::PublicKey> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }
}

pub(super) struct SshClientHandler {
    pub(super) host_key_capture: HostKeyCapture,
}

impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        self.host_key_capture.set(server_public_key);
        Ok(true)
    }
}

pub(super) enum AuthFailure {
    NeedsInput(BackendMessage),
    Fatal(BackendMessage),
}
