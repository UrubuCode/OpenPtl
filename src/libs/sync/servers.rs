use super::*;

/// Busca a lista oficial de servidores de autenticação publicada no
/// repositório.
///
/// É consultada a cada login para que uma troca de endereço ou de `client_id`
/// chegue aos aparelhos sem exigir atualização do aplicativo. O resultado é
/// mesclado com o que o usuário cadastrou: a lista oficial acrescenta e
/// atualiza, nunca apaga o que é local.
pub async fn fetch_official_servers() -> Result<Vec<AuthServer>> {
    let client = Client::builder()
        .timeout(AUTH_SERVERS_TIMEOUT)
        .build()
        .context("Falha ao preparar cliente HTTP")?;

    let response = client
        .get(AUTH_SERVERS_URL)
        .header("User-Agent", RELEASE_USER_AGENT)
        .send()
        .await
        .context("Falha ao consultar a lista oficial de servidores")?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("Lista oficial respondeu {}", status));
    }

    let servers: Vec<AuthServer> = response
        .json()
        .await
        .context("Lista oficial de servidores em formato inválido")?;

    Ok(servers.into_iter().filter_map(normalize).collect())
}

/// Descarta entradas malformadas em vez de deixar a lista inteira falhar: um
/// item quebrado no repositório não pode impedir o login.
fn normalize(mut server: AuthServer) -> Option<AuthServer> {
    server.id = server.id.trim().to_string();
    server.label = server.label.trim().to_string();
    server.address = server.address.trim().trim_end_matches('/').to_string();

    if server.id.is_empty() || server.label.is_empty() {
        return None;
    }
    if !server.address.starts_with("https://") && !server.address.starts_with("http://") {
        return None;
    }

    server.client_id = server
        .client_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    server.from_remote = true;
    Some(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, address: &str) -> AuthServer {
        AuthServer {
            id: id.to_string(),
            label: "OpenPtl".to_string(),
            address: address.to_string(),
            author: None,
            official: true,
            client_id: Some("  abc  ".to_string()),
            from_remote: false,
        }
    }

    #[test]
    fn a_valid_entry_is_marked_as_coming_from_the_official_list() {
        let normalized = normalize(server("default", "https://auth.exemplo/")).unwrap();
        assert!(normalized.from_remote);
        assert_eq!(normalized.address, "https://auth.exemplo");
        assert_eq!(normalized.client_id.as_deref(), Some("abc"));
    }

    #[test]
    fn an_entry_without_a_usable_address_is_dropped() {
        assert!(normalize(server("default", "ftp://auth")).is_none());
        assert!(normalize(server("", "https://auth")).is_none());
    }
}
