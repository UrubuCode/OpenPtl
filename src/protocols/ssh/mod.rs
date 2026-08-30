use std::{
    collections::HashMap,
    env, fs,
    io::{Read, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use russh::{
    client,
    keys::{
        self,
        known_hosts::{check_known_hosts_path, learn_known_hosts_path},
        PrivateKeyWithHashAlg, PublicKeyBase64,
    },
    ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect,
};
use russh_sftp::client::SftpSession;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    task::JoinHandle,
    time::sleep,
};

use crate::libs::models::{
    BackendMessage, ConnectionProfile, KnownHostEntry, SftpEntry, SshConnectPurpose,
    SshConnectResult, SshSessionInfo,
};

mod auth;
mod channel;
mod connection;
mod known_hosts;
mod local;
mod operations;
mod paths;
mod sftp_ops;
mod terminal;

pub(crate) use auth::*;
pub(crate) use channel::*;
pub(crate) use known_hosts::*;
pub(crate) use local::*;
pub(crate) use paths::*;
pub(crate) use terminal::*;

pub struct SshManager {
    sessions: HashMap<String, ManagedSession>,
    local_sessions: HashMap<String, LocalManagedSession>,
}

pub(crate) struct ManagedSession {
    info: SshSessionInfo,
    handle: client::Handle<SshClientHandler>,
    terminal: Option<TerminalSession>,
    sftp: Option<SftpSession>,
    mouse_sgr_enabled: bool,
}

pub(crate) struct TerminalSession {
    writer: Arc<ChannelWriteHalf<client::Msg>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader_task: JoinHandle<()>,
}

pub(crate) struct LocalManagedSession {
    info: SshSessionInfo,
    child: Child,
    stdin: ChildStdin,
    output: Arc<Mutex<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub(crate) struct HostKeyCapture {
    inner: Arc<Mutex<Option<keys::PublicKey>>>,
}

impl HostKeyCapture {
    pub(crate) fn set(&self, key: &keys::PublicKey) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(key.clone());
        }
    }

    pub(crate) fn get(&self) -> Option<keys::PublicKey> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }
}

pub(crate) struct SshClientHandler {
    host_key_capture: HostKeyCapture,
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

pub(crate) enum AuthFailure {
    NeedsInput(BackendMessage),
    Fatal(BackendMessage),
}
