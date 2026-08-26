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

mod connection;
mod known_hosts;
mod local;
mod operations;
mod session_types;
mod terminal;

use known_hosts::{
    authenticate_session, ensure_known_hosts_file, resolve_known_hosts_path, update_mouse_sgr_mode,
    verify_known_host,
};
pub use known_hosts::{known_hosts_add, known_hosts_ensure, known_hosts_list, known_hosts_remove};
use local::{
    auth_with_private_key, drain_local_output, join_remote_path, normalize_chunk_size,
    normalize_remote_path, pump_reader_into_buffer, spawn_local_shell,
};
use session_types::{
    AuthFailure, HostKeyCapture, LocalManagedSession, ManagedSession, SshClientHandler,
    TerminalSession,
};
use terminal::{
    drain_remote_output, ensure_sftp_session, open_terminal_session, run_remote_copy_command,
    write_to_remote_channel,
};

pub struct SshManager {
    sessions: HashMap<String, ManagedSession>,
    local_sessions: HashMap<String, LocalManagedSession>,
}
