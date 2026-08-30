use super::*;

pub fn known_hosts_list(path_override: Option<&str>) -> Result<Vec<KnownHostEntry>> {
    let path = resolve_known_hosts_path(path_override)?;
    ensure_known_hosts_file(&path)?;

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Falha ao ler known_hosts em {}", path.display()))?;

    let mut entries = Vec::new();
    for line in raw.lines() {
        if let Some(entry) = parse_known_host_line(line, &path) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub fn known_hosts_ensure(path_override: Option<&str>) -> Result<String> {
    let path = resolve_known_hosts_path(path_override)?;
    ensure_known_hosts_file(&path)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn known_hosts_remove(path_override: Option<&str>, line_raw: &str) -> Result<()> {
    let path = resolve_known_hosts_path(path_override)?;
    ensure_known_hosts_file(&path)?;

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Falha ao ler known_hosts em {}", path.display()))?;
    let target = line_raw.trim();

    let mut changed = false;
    let mut next_lines = Vec::new();
    for line in raw.lines() {
        if !changed && line.trim() == target {
            changed = true;
            continue;
        }
        next_lines.push(line);
    }

    if !changed {
        return Err(anyhow!("Entrada nao encontrada no known_hosts"));
    }

    let mut content = next_lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    fs::write(&path, content)
        .with_context(|| format!("Falha ao escrever known_hosts em {}", path.display()))
}

pub fn known_hosts_add(
    path_override: Option<&str>,
    host: &str,
    port: u16,
    key_type: &str,
    key_base64: &str,
) -> Result<KnownHostEntry> {
    let path = resolve_known_hosts_path(path_override)?;
    ensure_known_hosts_file(&path)?;

    let host_token = known_host_token(host, port);
    let line = format!("{} {} {}", host_token, key_type.trim(), key_base64.trim());

    let mut current = fs::read_to_string(&path)
        .with_context(|| format!("Falha ao ler known_hosts em {}", path.display()))?;
    if !current.ends_with('\n') && !current.is_empty() {
        current.push('\n');
    }
    current.push_str(&line);
    current.push('\n');
    fs::write(&path, current)
        .with_context(|| format!("Falha ao escrever known_hosts em {}", path.display()))?;

    parse_known_host_line(&line, &path)
        .ok_or_else(|| anyhow!("Falha ao montar entrada de known host"))
}

pub(crate) fn verify_known_host(
    server_key: &keys::PublicKey,
    profile: &ConnectionProfile,
    known_hosts_path: &Path,
    accept_unknown_host: bool,
) -> Result<Option<SshConnectResult>> {
    let key_type_label = server_key.algorithm().to_string();
    let fingerprint = host_fingerprint(&server_key.public_key_bytes());

    match check_known_hosts_path(&profile.host, profile.port, server_key, known_hosts_path) {
        Ok(true) => Ok(None),
        Ok(false) => {
            if !accept_unknown_host {
                return Ok(Some(SshConnectResult::UnknownHostChallenge {
                    host: profile.host.clone(),
                    port: profile.port,
                    key_type: key_type_label,
                    fingerprint,
                    known_hosts_path: known_hosts_path.to_string_lossy().to_string(),
                    message: BackendMessage::key("ssh_unknown_host_challenge"),
                }));
            }

            learn_known_hosts_path(&profile.host, profile.port, server_key, known_hosts_path)
                .context("Falha ao adicionar host ao known_hosts")?;
            Ok(None)
        }
        Err(keys::Error::KeyChanged { .. }) => Ok(Some(SshConnectResult::Error {
            message: BackendMessage::key("ssh_host_key_mismatch"),
        })),
        Err(error) => {
            let mut params = HashMap::new();
            params.insert("reason".to_string(), error.to_string());
            Ok(Some(SshConnectResult::Error {
                message: BackendMessage::with_params("ssh_known_hosts_validation_failed", params),
            }))
        }
    }
}

pub(crate) fn parse_known_host_line(line: &str, source_path: &Path) -> Option<KnownHostEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }

    let host_token = parts[0].split(',').next().unwrap_or(parts[0]);
    let (host, port) = parse_host_port(host_token);
    let key_type = parts[1].to_string();
    let fingerprint = decode_known_host_fingerprint(parts[2]);

    Some(KnownHostEntry {
        host,
        port,
        key_type,
        fingerprint,
        line_raw: trimmed.to_string(),
        path: source_path.to_string_lossy().to_string(),
    })
}

pub(crate) fn parse_host_port(host_token: &str) -> (String, u16) {
    if host_token.starts_with('[') {
        if let Some(close_idx) = host_token.find(']') {
            let host = host_token[1..close_idx].to_string();
            if let Some(port_text) = host_token.get(close_idx + 2..) {
                if let Ok(port) = port_text.parse::<u16>() {
                    return (host, port);
                }
            }
            return (host, 22);
        }
    }
    (host_token.to_string(), 22)
}

pub(crate) fn decode_known_host_fingerprint(base64_key: &str) -> String {
    match BASE64.decode(base64_key) {
        Ok(bytes) => host_fingerprint(&bytes),
        Err(_) => "-".to_string(),
    }
}

pub(crate) fn host_fingerprint(host_key: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(host_key);
    format!("SHA256:{}", BASE64.encode(hasher.finalize()))
}

pub(crate) fn known_host_token(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

pub(crate) fn ensure_known_hosts_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Falha ao criar diretorio de known_hosts: {}",
                parent.display()
            )
        })?;
    }
    if !path.exists() {
        fs::write(path, "")
            .with_context(|| format!("Falha ao criar arquivo known_hosts em {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn resolve_known_hosts_path(configured: Option<&str>) -> Result<PathBuf> {
    let from_settings = configured
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let path = if let Some(path) = from_settings {
        PathBuf::from(path)
    } else {
        default_known_hosts_path()?
    };

    if path.is_absolute() {
        Ok(path)
    } else {
        std::env::current_dir()
            .map_err(|error| anyhow!("Falha ao resolver diretorio atual: {}", error))
            .map(|cwd| cwd.join(path))
    }
}

pub(crate) fn default_known_hosts_path() -> Result<PathBuf> {
    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".ssh").join("known_hosts"));
    }
    if let Some(home) = env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(home).join(".ssh").join("known_hosts"));
    }
    Err(anyhow!(
        "Nao foi possivel descobrir HOME/USERPROFILE para known_hosts"
    ))
}
