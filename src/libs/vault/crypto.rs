use super::*;

/// Android/iOS have no such file, so this is desktop-only.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn legacy_known_hosts_content() -> Option<String> {
    let base = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let path = PathBuf::from(base).join(".ssh").join("known_hosts");
    fs::read_to_string(path)
        .ok()
        .filter(|content| !content.trim().is_empty())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn legacy_known_hosts_content() -> Option<String> {
    None
}

pub(crate) fn ensure_default_server(servers: &mut Vec<AuthServer>) {
    if !servers.iter().any(|item| item.id == "default") {
        servers.push(AuthServer::default_server());
    }
}

pub(crate) fn ensure_uuid(value: &str, kind: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value).is_err() {
        return Err(anyhow!("ID de {} invalido: {}", kind, value));
    }
    Ok(())
}

pub(crate) fn derive_key(password: &str, salt: &[u8; 16]) -> Result<[u8; 32]> {
    let params = Params::new(19_456, 3, 1, Some(32))
        .map_err(|error| anyhow!("Falha ao configurar parametros Argon2id: {}", error))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|error| anyhow!("Falha ao derivar chave via Argon2id: {}", error))?;
    Ok(key)
}

pub(crate) fn compute_key_check(key: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(b"openptl-key-check-v1");
    let digest = hasher.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

pub(crate) fn encode_bin<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|error| anyhow!("Falha ao serializar binario: {}", error))
}

pub(crate) fn decode_bin<T: DeserializeOwned>(bytes: &[u8], context: &str) -> Result<T> {
    bincode::serde::decode_from_slice(bytes, bincode::config::standard())
        .map(|(value, _)| value)
        .map_err(|error| anyhow!("{}: {}", context, error))
}

pub(crate) fn read_bin_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read(path).with_context(|| format!("Falha ao ler arquivo {}", path.display()))?;
    decode_bin(&raw, &format!("Falha ao decodificar {}", path.display()))
}

pub(crate) fn write_bin_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Falha ao criar diretorio {}", parent.display()))?;
    }
    let data = encode_bin(value)?;
    fs::write(path, data).with_context(|| format!("Falha ao escrever arquivo {}", path.display()))
}

/// Nonce sempre aleatório.
///
/// A versão anterior derivava o nonce do próprio conteúdo, o que revelava
/// quando dois arquivos guardavam a mesma coisa e não sobreviveria ao log de
/// mutações, onde lotes distintos podem ter conteúdo igual.
pub(crate) fn random_nonce() -> [u8; 24] {
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

pub(crate) fn encrypt_bin_payload<T: Serialize>(
    payload: &T,
    key: &[u8; 32],
    updated_at: i64,
) -> Result<EncryptedBin> {
    let plaintext = encode_bin(payload)?;
    let nonce = random_nonce();

    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| anyhow!("Falha ao criptografar payload"))?;

    Ok(EncryptedBin {
        version: CURRENT_STORAGE_VERSION,
        nonce,
        ciphertext,
        updated_at,
    })
}

pub(crate) fn decrypt_bin_payload<T: DeserializeOwned>(
    file: &EncryptedBin,
    key: &[u8; 32],
    context_message: &str,
) -> Result<T> {
    if file.version != CURRENT_STORAGE_VERSION {
        return Err(anyhow!(
            "Versao de arquivo nao suportada. Atual: {}, encontrada: {}",
            CURRENT_STORAGE_VERSION,
            file.version
        ));
    }

    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&file.nonce), file.ciphertext.as_ref())
        .map_err(|_| anyhow!("Falha ao descriptografar {}", context_message))?;

    decode_bin(
        &plaintext,
        &format!("Falha ao decodificar {}", context_message),
    )
}

/// Envelope dos objetos que vão para o Drive: `[nonce 24][ciphertext]`.
///
/// O corpo é JSON, não bincode: lotes e snapshots atravessam versões
/// diferentes do aplicativo, e um formato posicional faria um campo novo
/// invalidar o histórico já publicado pelos outros aparelhos.
pub fn encrypt_remote_blob<T: Serialize>(payload: &T, key: &[u8; 32]) -> Result<Vec<u8>> {
    let plaintext = serde_json::to_vec(payload).context("Falha ao serializar objeto remoto")?;
    let nonce = random_nonce();

    let cipher = XChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| anyhow!("Falha ao criptografar objeto remoto"))?;

    let mut out = Vec::with_capacity(nonce.len() + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt_remote_blob<T: DeserializeOwned>(bytes: &[u8], key: &[u8; 32]) -> Result<T> {
    if bytes.len() <= 24 {
        return Err(anyhow!("Objeto remoto truncado"));
    }
    let (nonce, ciphertext) = bytes.split_at(24);

    let cipher = XChaCha20Poly1305::new(key.into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow!("Falha ao descriptografar objeto remoto"))?;

    serde_json::from_slice(&plaintext).context("Falha ao decodificar objeto remoto")
}

pub(crate) fn normalize_bin_file_name(input: &str) -> Result<String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(anyhow!("Nome de arquivo vazio"));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(anyhow!("Nome de arquivo invalido"));
    }
    if !is_bin_file_name(value) {
        return Err(anyhow!("Apenas arquivos .bin sao permitidos"));
    }
    Ok(value.to_string())
}

pub(crate) fn is_bin_file_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .ends_with(&format!(".{}", STORAGE_FILE_EXTENSION))
}

pub(crate) fn persist_keychain_key(key: &[u8; 32]) -> Result<()> {
    secret_store::set(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY, &to_hex(key))
}

pub(crate) fn load_keychain_key() -> Result<[u8; 32]> {
    let value = secret_store::get(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY)
        .context("Nao foi possivel ler chave do keychain")?;

    let bytes = hex_to_bytes(&value)?;
    if bytes.len() != 32 {
        return Err(anyhow!("Chave do keychain invalida"));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

pub(crate) fn clear_keychain_key() {
    secret_store::delete(APP_KEYRING_SERVICE, KEYRING_VAULT_KEY);
}

pub(crate) fn hex_to_bytes(input: &str) -> Result<Vec<u8>> {
    let clean = input.trim();
    if !clean.len().is_multiple_of(2) {
        return Err(anyhow!("Hex invalido"));
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let chunk = std::str::from_utf8(&bytes[i..i + 2]).context("Hex invalido")?;
        let value = u8::from_str_radix(chunk, 16).context("Hex invalido")?;
        out.push(value);
    }
    Ok(out)
}

pub(crate) fn touch_local_change(payload: &mut VaultPayload) {
    payload.sync.last_local_change = Utc::now().timestamp();
}

pub(crate) fn normalize_option(input: Option<String>) -> Option<String> {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
