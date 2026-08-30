//! Interpretação dos endereços que abrem o aplicativo de fora.
//!
//! Três formas chegam na prática: o esquema próprio (`openptl://ssh/host:22`),
//! o esquema direto do protocolo (`ssh://usuario@host:22`) e o esquema próprio
//! embrulhando um endereço em parâmetro (`openptl://open?url=...`). Tudo que
//! não couber nessas formas é recusado em vez de virar um perfil inventado.

use crate::libs::models::ConnectionProtocol;

const DEFAULT_SSH_PORT: u16 = 22;
const OWN_SCHEME: &str = "openptl";

/// Alvo de conexão pedido por um endereço externo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLink {
    pub protocol: ConnectionProtocol,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
}

/// Profundidade máxima ao desembrulhar endereços aninhados. Um `openptl://`
/// apontando para outro `openptl://` em ciclo não pode travar o aplicativo.
const MAX_DEPTH: usize = 3;

pub fn parse(raw: &str) -> Option<DeepLink> {
    parse_at(raw, 0)
}

/// Primeiro argumento que se pareça com um endereço. É assim que o sistema
/// operacional entrega o clique num link para o processo.
pub fn from_arguments<I>(arguments: I) -> Option<DeepLink>
where
    I: IntoIterator<Item = String>,
{
    arguments
        .into_iter()
        .skip(1)
        .find_map(|argument| parse(&argument))
}

fn parse_at(raw: &str, depth: usize) -> Option<DeepLink> {
    if depth > MAX_DEPTH {
        return None;
    }

    let cleaned = normalize(raw);
    let (scheme, rest) = cleaned.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();

    match scheme.as_str() {
        "ssh" => direct(rest, ConnectionProtocol::Ssh),
        "sftp" => direct(rest, ConnectionProtocol::Sftp),
        OWN_SCHEME => own(rest, depth),
        _ => None,
    }
}

/// `openptl://ssh/host:22` ou `openptl://open?url=ssh%3A%2F%2Fhost`.
fn own(rest: &str, depth: usize) -> Option<DeepLink> {
    let (path, query) = split_query(rest);

    if let Some(embedded) = embedded_target(query) {
        return parse_at(&embedded, depth + 1);
    }

    let (head, tail) = path.split_once('/')?;
    let protocol = match head.to_ascii_lowercase().as_str() {
        "ssh" => ConnectionProtocol::Ssh,
        "sftp" => ConnectionProtocol::Sftp,
        _ => return None,
    };

    direct(tail.trim_start_matches('/'), protocol)
}

/// `usuario@host:porta`, com usuário e porta opcionais.
fn direct(rest: &str, protocol: ConnectionProtocol) -> Option<DeepLink> {
    let (authority, _) = split_query(rest);
    let authority = authority.trim_end_matches('/');
    if authority.is_empty() {
        return None;
    }

    let (username, host_port) = match authority.rsplit_once('@') {
        Some((user, host)) if !user.is_empty() => (Some(decode(user)), host),
        _ => (None, authority),
    };

    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (host_port, DEFAULT_SSH_PORT),
    };

    let host = host.trim();
    if host.is_empty() || port == 0 {
        return None;
    }

    Some(DeepLink {
        protocol,
        host: host.to_ascii_lowercase(),
        port,
        username,
    })
}

/// Alguns clientes de e-mail e terminais escapam o esquema ao repassar o link.
fn normalize(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .replace(":://", "://")
        .replace("%3A%2F%2F", "://")
        .replace("%3a%2f%2f", "://")
}

fn split_query(value: &str) -> (&str, &str) {
    match value.split_once('?') {
        Some((path, query)) => (path, query),
        None => (value, ""),
    }
}

fn embedded_target(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| matches!(*key, "url" | "target" | "uri" | "link"))
        .map(|(_, value)| decode(value))
}

/// Decodificação percentual suficiente para o que aparece num endereço.
fn decode(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_direct_ssh_scheme() {
        let link = parse("ssh://deploy@servidor.exemplo.com:2222").unwrap();
        assert_eq!(link.protocol, ConnectionProtocol::Ssh);
        assert_eq!(link.host, "servidor.exemplo.com");
        assert_eq!(link.port, 2222);
        assert_eq!(link.username.as_deref(), Some("deploy"));
    }

    #[test]
    fn port_defaults_to_ssh() {
        let link = parse("ssh://servidor.exemplo.com").unwrap();
        assert_eq!(link.port, DEFAULT_SSH_PORT);
        assert!(link.username.is_none());
    }

    #[test]
    fn parses_the_own_scheme() {
        let link = parse("openptl://sftp/deploy@host.local:22").unwrap();
        assert_eq!(link.protocol, ConnectionProtocol::Sftp);
        assert_eq!(link.host, "host.local");
        assert_eq!(link.username.as_deref(), Some("deploy"));
    }

    #[test]
    fn unwraps_an_embedded_address() {
        let link = parse("openptl://open?url=ssh%3A%2F%2Fdeploy%40host.local%3A2200").unwrap();
        assert_eq!(link.host, "host.local");
        assert_eq!(link.port, 2200);
        assert_eq!(link.username.as_deref(), Some("deploy"));
    }

    #[test]
    fn tolerates_the_doubled_colon_some_clients_produce() {
        let link = parse("openptl:://ssh/host.local").unwrap();
        assert_eq!(link.host, "host.local");
    }

    #[test]
    fn hosts_are_compared_in_lower_case() {
        let link = parse("ssh://Servidor.Exemplo.COM").unwrap();
        assert_eq!(link.host, "servidor.exemplo.com");
    }

    #[test]
    fn rejects_addresses_that_are_not_connections() {
        assert!(parse("https://exemplo.com").is_none());
        assert!(parse("openptl://configuracoes").is_none());
        assert!(parse("ssh://").is_none());
        assert!(parse("texto solto").is_none());
    }

    #[test]
    fn rejects_an_invalid_port() {
        assert!(parse("ssh://host.local:0").is_none());
        assert!(parse("ssh://host.local:porta").is_none());
        assert!(parse("ssh://host.local:99999").is_none());
    }

    #[test]
    fn nested_wrapping_cannot_loop_forever() {
        let looping = "openptl://open?url=openptl%3A%2F%2Fopen%3Furl%3Dopenptl%3A%2F%2Fopen";
        assert!(parse(looping).is_none());
    }

    #[test]
    fn takes_the_first_address_among_the_arguments() {
        let arguments = vec![
            "openptl.exe".to_owned(),
            "--flag".to_owned(),
            "ssh://host.local:22".to_owned(),
        ];
        let link = from_arguments(arguments).unwrap();
        assert_eq!(link.host, "host.local");
    }

    #[test]
    fn the_program_name_is_never_treated_as_an_address() {
        let arguments = vec!["ssh://malicioso".to_owned()];
        assert!(from_arguments(arguments).is_none());
    }
}
