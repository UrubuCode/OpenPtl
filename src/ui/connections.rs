use eframe::egui::{self, Align, Layout, RichText, Stroke};

use crate::app::{OpenPtlApp, Screen};
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
            ui.columns(2, |columns| {
                render_list(app, &mut columns[0]);
                render_editor(app, &mut columns[1]);
            });
        });
}

fn render_list(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Perfis salvos",
        "Selecione um perfil para editar ou iniciar uma sessão.",
        |ui| {
            if app.connections.is_empty() {
                components::empty_state(
                    ui,
                    "Nenhum perfil salvo",
                    "Use o botão acima para adicionar uma conexão.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(540.0)
                .show(ui, |ui| {
                    for profile in app.connections.clone() {
                        profile_card(app, ui, &profile);
                    }
                });
        },
    );
}

fn profile_card(app: &mut OpenPtlApp, ui: &mut egui::Ui, profile: &ConnectionProfile) {
    let selected = app.editing_connection_id.as_deref() == Some(profile.id.as_str());
    let fill = if selected {
        theme::ACCENT_DARK
    } else {
        theme::PANEL_RAISED
    };
    let border = if selected {
        theme::ACCENT
    } else {
        theme::BORDER
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&profile.name).strong());
                    ui.label(
                        RichText::new(format!(
                            "{}@{}:{}",
                            profile.username, profile.host, profile.port
                        ))
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(4.0);
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
            ui.add_space(10.0);
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

fn render_editor(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    let title = if app.editing_connection_id.is_some() {
        "Editar conexão"
    } else {
        "Nova conexão"
    };
    components::section(
        ui,
        title,
        "Os dados são armazenados somente após salvar no vault.",
        |ui| {
            egui::ScrollArea::vertical()
                .max_height(540.0)
                .show(ui, |ui| {
                    app.connection_form.render(ui);
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if components::primary_button(ui, "Salvar conexão").clicked() {
                            app.save_connection();
                        }
                        if let Some(id) = app.editing_connection_id.clone() {
                            if components::danger_button(ui, "Excluir").clicked() {
                                app.request_delete_connection(&id);
                            }
                        }
                        if components::secondary_button(ui, "Workspace").clicked() {
                            app.screen = Screen::Workspace;
                        }
                    });
                });
        },
    );
}

fn reset_editor(app: &mut OpenPtlApp) {
    app.connection_form = super::connection_form::ConnectionForm::default();
    app.editing_connection_id = None;
    app.screen = Screen::Connections;
}

fn has_protocol(profile: &ConnectionProfile, target: ConnectionProtocol) -> bool {
    profile.protocols.contains(&target)
}
