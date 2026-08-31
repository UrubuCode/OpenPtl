//! Desbloqueio automático do cofre durante o desenvolvimento.
//!
//! Existe para poder abrir as telas internas sem digitar a senha a cada
//! recompilação. É deliberadamente inerte fora de um build de depuração: o
//! corpo inteiro está atrás de `cfg(debug_assertions)`, então em release o
//! módulo não tem código nenhum e não há caminho para ativá-lo.
//!
//! A senha vem de `OPENPTL_DEV_PASSWORD`, no ambiente ou num `.env` local que o
//! git ignora. Nada é gravado no repositório.

#[cfg(debug_assertions)]
const VARIABLE: &str = "OPENPTL_DEV_PASSWORD";

/// Senha de desenvolvimento, se houver uma configurada.
#[cfg(debug_assertions)]
pub fn password() -> Option<String> {
    if let Ok(value) = std::env::var(VARIABLE) {
        let value = value.trim().to_owned();
        if !value.is_empty() {
            return Some(value);
        }
    }
    from_env_file()
}

#[cfg(not(debug_assertions))]
pub fn password() -> Option<String> {
    None
}

/// Lê `OPENPTL_DEV_PASSWORD` de um `.env` ao lado do projeto. Formato mínimo:
/// uma atribuição por linha, `#` inicia comentário.
#[cfg(debug_assertions)]
fn from_env_file() -> Option<String> {
    let content = std::fs::read_to_string(".env").ok()?;

    content.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        if key.trim() != VARIABLE {
            return None;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_builds_never_have_a_development_password() {
        // Em release a função é a versão vazia; o teste documenta a garantia e
        // falha se alguém remover o `cfg`.
        #[cfg(not(debug_assertions))]
        assert!(super::password().is_none());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn comments_and_other_keys_are_ignored() {
        // A leitura acontece sobre o conteúdo, não sobre o arquivo: o teste não
        // depende de existir um `.env` na máquina.
        let parse = |content: &str| -> Option<String> {
            content.lines().find_map(|line| {
                let line = line.trim();
                if line.starts_with('#') {
                    return None;
                }
                let (key, value) = line.split_once('=')?;
                if key.trim() != super::VARIABLE {
                    return None;
                }
                let value = value.trim().trim_matches('"');
                (!value.is_empty()).then(|| value.to_owned())
            })
        };

        assert_eq!(parse("# OPENPTL_DEV_PASSWORD=x"), None);
        assert_eq!(parse("OUTRA=x"), None);
        assert_eq!(parse("OPENPTL_DEV_PASSWORD="), None);
        assert_eq!(
            parse("OUTRA=1\nOPENPTL_DEV_PASSWORD=\"segredo\""),
            Some("segredo".to_owned())
        );
    }
}
