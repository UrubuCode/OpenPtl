use eframe::egui::{self, Align, Id, Layout, RichText, Stroke};

use crate::app::OpenPtlApp;
use crate::libs::models::{ConnectionProfile, ConnectionProtocol, SshConnectPurpose};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            ui.horizontal(|ui| {
                components::page_header(
                    ui,
                    "Gerenciamento",
                    "Conexões",
                    "Perfis SSH e SFTP protegidos pelo vault.",
                );
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    if components::primary_button(ui, "＋  Nova conexão").clicked() {
                        reset_editor(app);
                    }
                });
            });
            components::section(
                ui,
                "Perfis salvos",
                "Selecione um perfil para conectar ou editar.",
                |ui| {
                    if app.connections.is_empty() {
                        components::empty_state(
                            ui,
                            "Nenhum perfil salvo",
                            "Crie seu primeiro perfil para iniciar uma sessão remota.",
                        );
                        return;
                    }
                    egui::ScrollArea::vertical()
                        .max_height(620.0)
                        .show(ui, |ui| {
                            for profile in app.connections.clone() {
                                profile_card(app, ui, &profile);
                            }
                        });
                },
            );
        });
    render_editor_modal(app, context);
}

fn profile_card(app: &mut OpenPtlApp, ui: &mut egui::Ui, profile: &ConnectionProfile) {
    egui::Frame::new()
        .fill(theme::PANEL_RAISED)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&profile.name).size(16.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "{}@{}:{}",
                            profile.username, profile.host, profile.port
                        ))
                        .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        for protocol in &profile.protocols {
                            let (label, color) = match protocol {
                                ConnectionProtocol::Ssh => ("SSH", theme::ACCENT),
                                ConnectionProtocol::Sftp => ("SFTP", theme::SUCCESS),
                                _ => ("LEGADO", theme::TEXT_MUTED),
                            };
                            components::badge(ui, label, color);
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if components::secondary_button(ui, "Editar").clicked() {
                        app.start_editing(&profile.id);
                    }
                });
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if has_protocol(profile, ConnectionProtocol::Ssh)
                    && components::primary_button(ui, "Conectar via SSH").clicked()
                {
                    app.connect(&profile.id, SshConnectPurpose::Terminal, false);
                }
                if has_protocol(profile, ConnectionProtocol::Sftp)
                    && components::secondary_button(ui, "Abrir SFTP").clicked()
                {
                    app.connect(&profile.id, SshConnectPurpose::Sftp, false);
                }
            });
        });
    ui.add_space(8.0);
}

fn render_editor_modal(app: &mut OpenPtlApp, context: &egui::Context) {
    if !app.show_connection_editor {
        return;
    }
    let mut close = false;
    let title = if app.editing_connection_id.is_some() {
        "Editar conexão"
    } else {
        "Nova conexão"
    };
    let modal_response = egui::Modal::new(Id::new("connection_editor_modal"))
        .backdrop_color(egui::Color32::from_black_alpha(165))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(20)),
        )
        .show(context, |ui| {
            ui.set_min_width(720.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(21.0).strong());
                    ui.label(
                        RichText::new("As alterações serão gravadas no vault criptografado.")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    if ui
                        .button(RichText::new("×").size(20.0).color(theme::TEXT_MUTED))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            ui.add_space(16.0);
            app.connection_form.render(ui);
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if components::secondary_button(ui, "Cancelar").clicked() {
                    close = true;
                }
                if components::primary_button(ui, "Salvar conexão").clicked() {
                    app.save_connection();
                }
            });
        });
    if close
        || modal_response.backdrop_response.clicked()
        || context.input(|input| input.key_pressed(egui::Key::Escape))
    {
        app.show_connection_editor = false;
    }
}

fn reset_editor(app: &mut OpenPtlApp) {
    app.connection_form = super::connection_form::ConnectionForm::default();
    app.editing_connection_id = None;
    app.show_connection_editor = true;
}

fn has_protocol(profile: &ConnectionProfile, target: ConnectionProtocol) -> bool {
    profile.protocols.contains(&target)
}
