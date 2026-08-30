use super::*;

pub(crate) async fn authenticate_session(
    handle: &mut client::Handle<SshClientHandler>,
    profile: &ConnectionProfile,
) -> Result<(), AuthFailure> {
    let key_data = profile
        .private_key
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    let password_data = profile
        .password
        .as_deref()
        .filter(|value| !value.trim().is_empty());

    if key_data.is_none() && password_data.is_none() {
        return Err(AuthFailure::NeedsInput(BackendMessage::key(
            "ssh_credentials_missing",
        )));
    }

    if let Some(private_key) = key_data {
        let key_result =
            auth_with_private_key(handle, &profile.username, private_key, password_data).await;

        if let Ok(true) = key_result {
            return Ok(());
        }

        if password_data.is_none() {
            let message = match key_result {
                Ok(false) => BackendMessage::key("ssh_private_key_auth_failed"),
                Ok(true) => BackendMessage::key("ssh_private_key_auth_unexpected"),
                Err(error) => {
                    let mut params = HashMap::new();
                    params.insert("reason".to_string(), error.to_string());
                    BackendMessage::with_params("ssh_private_key_auth_failed_with_reason", params)
                }
            };
            return Err(AuthFailure::NeedsInput(message));
        }
    }

    if let Some(password) = password_data {
        let auth = handle
            .authenticate_password(&profile.username, password)
            .await
            .map_err(|error| {
                let mut params = HashMap::new();
                params.insert("reason".to_string(), error.to_string());
                AuthFailure::NeedsInput(BackendMessage::with_params(
                    "ssh_password_auth_failed_with_reason",
                    params,
                ))
            })?;

        if auth.success() {
            return Ok(());
        }

        return Err(AuthFailure::NeedsInput(BackendMessage::key(
            "ssh_password_auth_failed",
        )));
    }

    Err(AuthFailure::Fatal(BackendMessage::key("ssh_auth_failed")))
}

pub(crate) async fn auth_with_private_key(
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
