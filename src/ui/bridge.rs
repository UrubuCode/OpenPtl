//! Ponte entre os callbacks declarados em `ui/main.slint` e o estado Rust.
//!
//! A UI só emite intenções; toda regra de negócio vive nos módulos de domínio.
//! Cada callback é registrado uma vez em `run` e usa um handle fraco da janela
//! para evitar o ciclo de referência que vazaria a `AppWindow`.

use anyhow::Result;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::{AppWindow, ConnectionRow, VaultState};

pub fn run() -> Result<()> {
    let window = AppWindow::new()?;

    window.set_vault(VaultState {
        initialized: false,
        locked: true,
        recoverable: false,
    });
    window.set_connections(ModelRc::new(VecModel::<ConnectionRow>::default()));

    bind_vault(&window);
    bind_connections(&window);

    window.run()?;
    Ok(())
}

fn bind_vault(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_vault_submitted(move |password, confirmation| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let state = window.get_vault();

        if !state.initialized && password != confirmation {
            window.set_vault_error("As senhas não coincidem.".into());
            return;
        }
        if password.len() < MIN_PASSWORD_LEN {
            window.set_vault_error(
                format!("A senha mestre precisa de ao menos {MIN_PASSWORD_LEN} caracteres.").into(),
            );
            return;
        }

        window.set_vault_error(Default::default());
        window.set_vault_busy(true);
    });
}

fn bind_connections(window: &AppWindow) {
    let handle = window.as_weak();
    window.on_connection_delete_requested(move |id| {
        let Some(window) = handle.upgrade() else {
            return;
        };
        let rows = window.get_connections();
        let kept: Vec<ConnectionRow> = rows.iter().filter(|row| row.id != id).collect();
        window.set_connections(ModelRc::new(VecModel::from(kept)));
    });
}

const MIN_PASSWORD_LEN: usize = 6;
