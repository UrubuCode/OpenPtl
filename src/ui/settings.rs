use eframe::egui::{self, RichText};

use crate::app::OpenPtlApp;
use crate::libs::models::{AppSettings, EditorPreference, ModifiedUploadPolicy};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            components::page_header(
                ui,
                "Preferências",
                "Configurações",
                "Ajuste o comportamento do cliente sem sair do vault protegido.",
            );
            if !app.settings_loaded {
                let loaded = app
                    .backend
                    .as_ref()
                    .and_then(|backend| backend.settings().ok());
                if let Some(settings) = loaded {
                    app.settings = settings;
                    app.settings_loaded = true;
                } else {
                    components::empty_state(
                        ui,
                        "Configurações indisponíveis",
                        "Desbloqueie o vault para carregar suas preferências.",
                    );
                    return;
                }
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                render_editor_settings(&mut app.settings, ui);
                ui.add_space(12.0);
                render_behavior_settings(&mut app.settings, ui);
                ui.add_space(12.0);
                render_security_settings(&mut app.settings, ui);
                ui.add_space(16.0);
                if components::primary_button(ui, "Salvar configurações").clicked() {
                    save_settings(app);
                }
            });
        });
}

fn render_editor_settings(settings: &mut AppSettings, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Editor",
        "Defina como arquivos remotos devem ser abertos.",
        |ui| {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    components::field_label(ui, "Editor preferido", "Usado em ações de edição.");
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
                });
                columns[1].vertical(|ui| {
                    components::field_label(ui, "Comando externo", "Ex.: code --wait");
                    ui.text_edit_singleline(&mut settings.external_editor_command);
                });
            });
        },
    );
}

fn render_behavior_settings(settings: &mut AppSettings, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Comportamento",
        "Automatize sincronização e reconexão quando fizer sentido.",
        |ui| {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    ui.checkbox(&mut settings.sync_auto_enabled, "Sincronização automática");
                    ui.checkbox(&mut settings.sync_on_startup, "Sincronizar ao iniciar");
                    ui.checkbox(
                        &mut settings.sync_on_settings_change,
                        "Sincronizar ao salvar mudanças",
                    );
                });
                columns[1].vertical(|ui| {
                    ui.checkbox(&mut settings.auto_reconnect_enabled, "Reconexão automática");
                    components::field_label(ui, "Intervalo de sincronização", "Minutos");
                    ui.add(egui::DragValue::new(&mut settings.sync_interval_minutes).range(1..=60));
                });
            });
        },
    );
}

fn render_security_settings(settings: &mut AppSettings, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Rede e segurança",
        "Limites operacionais para conexões e transferências.",
        |ui| {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    components::field_label(ui, "Bloco SFTP", "Tamanho em KB");
                    ui.add(egui::DragValue::new(&mut settings.sftp_chunk_size_kb).range(64..=8192));
                    components::field_label(ui, "Bloqueio por inatividade", "Minutos");
                    ui.add(
                        egui::DragValue::new(&mut settings.inactivity_lock_minutes).range(1..=240),
                    );
                });
                columns[1].vertical(|ui| {
                    components::field_label(
                        ui,
                        "Upload de arquivo modificado",
                        "Quando o destino já possui uma versão",
                    );
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
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Os segredos continuam dentro do vault criptografado.")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
        },
    );
}

fn save_settings(app: &mut OpenPtlApp) {
    let result = app
        .backend
        .as_mut()
        .map(|backend| backend.update_settings(app.settings.clone()));
    match result {
        Some(Ok(_)) => {
            app.message = Some("Configurações salvas no vault.".to_string());
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
