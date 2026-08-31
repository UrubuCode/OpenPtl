//! Fluxo de configurações e bloqueio manual do cofre.
//!
//! O rascunho da interface cobre apenas as preferências cujo comportamento já
//! existe no binário nativo. As demais continuam guardadas no cofre intactas:
//! salvar aqui nunca as sobrescreve com valores inventados.

use std::sync::Arc;

use slint::ComponentHandle;

use super::{AppWindow, SettingsDraft};
use crate::backend::Backend;
use crate::libs::models::AppSettings;

pub fn bind(window: &AppWindow, backend: Arc<Backend>) {
    bind_master_password(window, Arc::clone(&backend));
    let handle = window.as_weak();
    let saving = Arc::clone(&backend);
    window.on_settings_saved(move |draft| {
        let Some(window) = handle.upgrade() else {
            return;
        };

        let outcome = saving
            .settings()
            .and_then(|current| saving.settings_update(merge(current, &draft)));

        match outcome {
            Ok(saved) => {
                window.set_settings(to_draft(&saved));
                window.set_settings_message("Preferências salvas.".into());
            }
            Err(error) => window.set_settings_message(format!("{error}").into()),
        }
    });

    let handle = window.as_weak();
    window.on_vault_lock_requested(move || {
        let Some(window) = handle.upgrade() else {
            return;
        };
        match backend.lock() {
            Ok(status) => super::bridge::apply_vault_status(&window, status),
            Err(error) => window.set_settings_message(format!("{error}").into()),
        }
    });
}

pub fn refresh(window: &AppWindow, backend: &Backend) {
    match backend.settings() {
        Ok(settings) => window.set_settings(to_draft(&settings)),
        Err(error) => window.set_settings_message(format!("{error}").into()),
    }
}

fn to_draft(settings: &AppSettings) -> SettingsDraft {
    SettingsDraft {
        inactivity_lock_minutes: settings.inactivity_lock_minutes.to_string().into(),
        auto_reconnect_enabled: settings.auto_reconnect_enabled,
        reconnect_delay_seconds: settings.reconnect_delay_seconds.to_string().into(),
        terminal_copy_on_select: settings.terminal_copy_on_select,
        terminal_right_click_paste: settings.terminal_right_click_paste,
        terminal_ctrl_shift_shortcuts: settings.terminal_ctrl_shift_shortcuts,
    }
}

/// Aplica o rascunho sobre as configurações vindas do cofre, preservando todo
/// campo que a interface ainda não expõe.
fn merge(mut settings: AppSettings, draft: &SettingsDraft) -> AppSettings {
    settings.inactivity_lock_minutes = parse_or_keep(
        &draft.inactivity_lock_minutes,
        settings.inactivity_lock_minutes,
    );
    settings.auto_reconnect_enabled = draft.auto_reconnect_enabled;
    settings.reconnect_delay_seconds = parse_or_keep(
        &draft.reconnect_delay_seconds,
        settings.reconnect_delay_seconds,
    );
    settings.terminal_copy_on_select = draft.terminal_copy_on_select;
    settings.terminal_right_click_paste = draft.terminal_right_click_paste;
    settings.terminal_ctrl_shift_shortcuts = draft.terminal_ctrl_shift_shortcuts;
    settings
}

/// Texto inválido mantém o valor atual em vez de zerar a preferência.
fn parse_or_keep(value: &slint::SharedString, current: u32) -> u32 {
    value.trim().parse().unwrap_or(current)
}

/// Troca de senha mestre. O resultado aparece na própria seção de segurança,
/// não numa mensagem global: é ali que o usuário está olhando.
fn bind_master_password(window: &AppWindow, backend: Arc<Backend>) {
    let handle = window.as_weak();
    window.on_master_password_changed(move |current, next, confirmation| {
        let Some(window) = handle.upgrade() else {
            return;
        };

        match backend.change_master_password(&current, &next, &confirmation) {
            Ok(()) => window.set_password_message("Senha mestre atualizada.".into()),
            Err(error) => window.set_password_message(format!("{error}").into()),
        }
    });
}
