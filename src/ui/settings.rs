use eframe::egui;

use crate::app::OpenPtlApp;
use crate::libs::models::{AppSettings, EditorPreference, ModifiedUploadPolicy};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.heading("Configurações");
        ui.label("Preferências persistidas junto ao vault criptografado.");
        ui.add_space(14.0);

        if !app.settings_loaded {
            let loaded = app
                .backend
                .as_ref()
                .and_then(|backend| backend.settings().ok());
            if let Some(settings) = loaded {
                app.settings = settings;
                app.settings_loaded = true;
            } else {
                ui.label(egui::RichText::new("Não foi possível carregar as configurações.").weak());
                return;
            }
        }

        render_editor(ui, &mut app.settings);
        ui.add_space(12.0);
        if ui.button("Salvar configurações").clicked() {
            let result = app
                .backend
                .as_mut()
                .map(|backend| backend.update_settings(app.settings.clone()));
            match result {
                Some(Ok(_)) => {
                    app.message = Some("Configurações salvas.".to_string());
                    app.message_is_error = false;
                }
                Some(Err(error)) => {
                    app.message = Some(error.to_string());
                    app.message_is_error = true;
                }
                None => {
                    app.message = Some("Backend indisponível.".to_string());
                    app.message_is_error = true;
                }
            }
        }
    });
}

fn render_editor(ui: &mut egui::Ui, settings: &mut AppSettings) {
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([18.0, 12.0])
        .show(ui, |ui| {
            ui.label("Editor preferido");
            egui::ComboBox::from_id_salt("editor_preference")
                .selected_text(editor_label(&settings.preferred_editor))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut settings.preferred_editor,
                        EditorPreference::Internal,
                        "Interno",
                    );
                    ui.selectable_value(
                        &mut settings.preferred_editor,
                        EditorPreference::Vscode,
                        "VS Code",
                    );
                    ui.selectable_value(
                        &mut settings.preferred_editor,
                        EditorPreference::System,
                        "Sistema",
                    );
                });
            ui.end_row();
            ui.label("Comando de editor externo");
            ui.text_edit_singleline(&mut settings.external_editor_command);
            ui.end_row();
            ui.label("Sincronização automática");
            ui.checkbox(&mut settings.sync_auto_enabled, "Ativada");
            ui.end_row();
            ui.label("Sincronizar ao iniciar");
            ui.checkbox(&mut settings.sync_on_startup, "Ativada");
            ui.end_row();
            ui.label("Intervalo de sync (minutos)");
            ui.add(egui::DragValue::new(&mut settings.sync_interval_minutes).range(1..=60));
            ui.end_row();
            ui.label("Tamanho de bloco SFTP (KB)");
            ui.add(egui::DragValue::new(&mut settings.sftp_chunk_size_kb).range(64..=8192));
            ui.end_row();
            ui.label("Bloqueio por inatividade (minutos)");
            ui.add(egui::DragValue::new(&mut settings.inactivity_lock_minutes).range(1..=240));
            ui.end_row();
            ui.label("Reconexão automática");
            ui.checkbox(&mut settings.auto_reconnect_enabled, "Ativada");
            ui.end_row();
            ui.label("Política de upload modificado");
            egui::ComboBox::from_id_salt("upload_policy")
                .selected_text(policy_label(&settings.modified_files_upload_policy))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut settings.modified_files_upload_policy,
                        ModifiedUploadPolicy::Auto,
                        "Automático",
                    );
                    ui.selectable_value(
                        &mut settings.modified_files_upload_policy,
                        ModifiedUploadPolicy::Ask,
                        "Perguntar",
                    );
                    ui.selectable_value(
                        &mut settings.modified_files_upload_policy,
                        ModifiedUploadPolicy::Manual,
                        "Manual",
                    );
                });
            ui.end_row();
        });
}

fn editor_label(editor: &EditorPreference) -> &'static str {
    match editor {
        EditorPreference::Internal => "Interno",
        EditorPreference::Vscode => "VS Code",
        EditorPreference::System => "Sistema",
    }
}

fn policy_label(policy: &ModifiedUploadPolicy) -> &'static str {
    match policy {
        ModifiedUploadPolicy::Auto => "Automático",
        ModifiedUploadPolicy::Ask => "Perguntar",
        ModifiedUploadPolicy::Manual => "Manual",
    }
}
