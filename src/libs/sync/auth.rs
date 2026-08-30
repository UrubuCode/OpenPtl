use super::*;

pub(crate) fn finalize_auth_result(
    reporter: &Reporter,
    result: std::result::Result<Result<CallbackAuthData>, tokio::time::error::Elapsed>,
) -> Result<SyncState> {
    let state = match result {
        Ok(Ok(auth_data)) => {
            store_refresh_token(&auth_data.refresh_token)?;
            if let Some(ref email) = auth_data.email {
                store_user_field(KEYRING_USER_EMAIL, email).ok();
            }
            if let Some(ref name) = auth_data.name {
                store_user_field(KEYRING_USER_NAME, name).ok();
            }
            if let Some(ref picture_url) = auth_data.picture_url {
                store_user_field(KEYRING_USER_PICTURE, picture_url).ok();
            }

            let message = if let Some(ref name) = auth_data.name {
                let email = auth_data.email.as_deref().unwrap_or("");
                let mut params = HashMap::new();
                params.insert("name".to_string(), name.clone());
                params.insert("email".to_string(), email.to_string());
                BackendMessage::with_params("sync_login_connected_as", params)
            } else {
                BackendMessage::key("sync_login_connected")
            };

            SyncState::ok(message, None)
        }
        Ok(Err(error)) => {
            let mut params = HashMap::new();
            params.insert("reason".to_string(), format!("{}", error));
            SyncState::error(BackendMessage::with_params("sync_login_error", params))
        }
        Err(_) => SyncState::error("sync_login_timeout"),
    };

    reporter.status(state.clone());
    Ok(state)
}

pub(crate) fn derive_aes_key(client_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(client_id.as_bytes());
    hasher.finalize().into()
}

pub(crate) fn decrypt_callback_data(
    encrypted_b64: &str,
    client_id: &str,
) -> Result<CallbackAuthData> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encrypted_b64)
        .context("base64 invalido no callback")?;

    if bytes.len() < 12 + 16 {
        return Err(anyhow!("payload de callback muito curto"));
    }

    let (iv_bytes, ciphertext) = bytes.split_at(12);
    let key_bytes = derive_aes_key(client_id);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(iv_bytes), ciphertext)
        .map_err(|_| anyhow!("falha ao descriptografar callback de auth"))?;

    serde_json::from_slice(&plaintext).context("JSON invalido no callback decriptado")
}

pub(crate) fn parse_auth_callback_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            Some((
                urlencoding::decode(key).unwrap_or_default().to_string(),
                urlencoding::decode(value).unwrap_or_default().to_string(),
            ))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct CallbackAuthData {
    refresh_token: String,
    email: Option<String>,
    name: Option<String>,
    picture_url: Option<String>,
}

pub(crate) async fn wait_for_callback(listener: &TcpListener) -> Result<CallbackAuthData> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("Falha ao aceitar conexao")?;

    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .context("Falha ao ler request")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    let query = path.split('?').nth(1).unwrap_or("");
    let params = parse_auth_callback_query(query);

    if let Some(error) = params.get("error") {
        let html = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n\
            <html><body><h2>Erro no login</h2><p>{}</p>\
            <script>setTimeout(()=>window.close(),2000)</script></body></html>",
            error
        );
        stream.write_all(html.as_bytes()).await.ok();
        stream.flush().await.ok();
        return Err(anyhow!("Login falhou: {}", error));
    }

    // Encrypted path: single `data` param
    let auth_data = if let Some(encrypted) = params.get("data").filter(|v| !v.is_empty()) {
        let client_id = take_pending_client_id()
            .ok_or_else(|| anyhow!("client_id nao disponivel para descriptografar callback"))?;
        decrypt_callback_data(encrypted, &client_id)?
    } else {
        // Legacy plain-text path
        let refresh_token = params
            .get("refresh_token")
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or_else(|| anyhow!("refresh_token nao recebido no callback"))?;
        CallbackAuthData {
            refresh_token,
            email: params.get("email").cloned().filter(|v| !v.is_empty()),
            name: params.get("name").cloned().filter(|v| !v.is_empty()),
            picture_url: params
                .get("picture")
                .or_else(|| params.get("picture_url"))
                .cloned()
                .filter(|v| !v.is_empty()),
        }
    };

    let display_name = auth_data.name.as_deref().unwrap_or("usuario");
    let html = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
        <html><body style=\"font-family:system-ui;text-align:center;padding:60px\">\
        <h2>Conectado como {}</h2>\
        <p>Pode fechar esta janela.</p>\
        <script>setTimeout(()=>window.close(),2000)</script></body></html>",
        display_name
    );
    stream.write_all(html.as_bytes()).await.ok();
    stream.flush().await.ok();

    Ok(auth_data)
}

pub(crate) async fn access_token_from_refresh_with_fallback(
    primary: &str,
    fallbacks: &[String],
) -> Result<String> {
    match try_refresh_token(primary).await {
        Ok(token) => Ok(token),
        Err(e) => {
            if e.to_string().contains("401") || e.to_string().contains("Execute login") {
                return Err(e);
            }
            for fallback in fallbacks {
                if fallback == primary {
                    continue;
                }
                if let Ok(token) = try_refresh_token(fallback).await {
                    return Ok(token);
                }
            }
            Err(e)
        }
    }
}

pub(crate) async fn try_refresh_token(server_address: &str) -> Result<String> {
    let refresh_token = load_refresh_token()?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| Client::new());

    let response = client
        .post(format!("{}/auth/refresh-token", server_address))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .context("Falha ao renovar access token via worker")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Falha ao renovar token ({}). Execute login novamente. {}",
            status,
            body
        ));
    }

    let data: RefreshTokenResponse = response
        .json()
        .await
        .context("Falha ao ler resposta de refresh")?;

    Ok(data.access_token)
}

pub(crate) fn store_refresh_token(token: &str) -> Result<()> {
    secret_store::set(APP_KEYRING_SERVICE, KEYRING_REFRESH_TOKEN, token)
        .context("Falha ao salvar refresh token no keychain")
}

pub(crate) fn load_refresh_token() -> Result<String> {
    secret_store::get(APP_KEYRING_SERVICE, KEYRING_REFRESH_TOKEN)
        .context("Refresh token ausente. Faca login primeiro.")
}

pub(crate) fn store_user_field(key: &str, value: &str) -> Result<()> {
    secret_store::set(APP_KEYRING_SERVICE, key, value).context("Falha ao salvar dado no keychain")
}

pub(crate) fn load_user_field(key: &str) -> Result<String> {
    secret_store::get(APP_KEYRING_SERVICE, key).context("Campo ausente no keychain")
}

pub(crate) fn delete_keyring_field(key: &str) {
    secret_store::delete(APP_KEYRING_SERVICE, key);
}

#[derive(Debug, Deserialize)]
pub(crate) struct RefreshTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DriveFileListResponse {
    files: Vec<DriveFileMetadata>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct DriveFileMetadata {
    id: String,
    name: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

/// Abre o navegador do sistema no endereço de login. O Tauri fazia isso por um
/// plugin; aqui é o comando padrão do sistema operacional.
pub(crate) fn open_login_url(url: &str) -> Result<()> {
    open::that_detached(url).with_context(|| format!("Falha ao abrir o navegador em {url}"))
}
