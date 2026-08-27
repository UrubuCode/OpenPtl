use eframe::egui::{self, RichText};

use crate::app::{OpenPtlApp, Screen};
use crate::libs::models::SshConnectPurpose;

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            components::page_header(
                ui,
                "Visão geral",
                "Seu espaço de trabalho",
                "Acesse conexões, sessões e credenciais a partir de um único lugar.",
            );
            render_metrics(app, ui);
            ui.add_space(18.0);
            render_quick_actions(app, ui);
            ui.add_space(18.0);
            render_saved_connections(app, ui);
        });
}

fn render_metrics(app: &OpenPtlApp, ui: &mut egui::Ui) {
    ui.columns(3, |columns| {
        metric_card(
            &mut columns[0],
            "Conexões",
            app.connections.len(),
            "Perfis protegidos",
            theme::ACCENT,
        );
        metric_card(
            &mut columns[1],
            "Sessões",
            app.sessions.len(),
            "Terminais ativos",
            theme::SUCCESS,
        );
        metric_card(
            &mut columns[2],
            "Credenciais",
            app.keychain.len(),
            "Itens protegidos",
            theme::WARNING,
        );
    });
}

fn metric_card(
    ui: &mut egui::Ui,
    title: &str,
    value: usize,
    description: &str,
    color: egui::Color32,
) {
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(color, "●");
            ui.label(RichText::new(title).strong().color(theme::TEXT_MUTED));
        });
        ui.add_space(6.0);
        ui.label(RichText::new(value.to_string()).size(30.0).strong());
        ui.label(RichText::new(description).small().color(theme::TEXT_MUTED));
    });
}

fn render_quick_actions(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Ações rápidas",
        "Comece uma nova sessão ou organize seus perfis.",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                if components::primary_button(ui, "＋  Nova conexão").clicked() {
                    app.connection_form = super::connection_form::ConnectionForm::default();
                    app.editing_connection_id = None;
                    app.show_connection_editor = true;
                    app.screen = Screen::Connections;
                }
                if components::secondary_button(ui, "⌁  Abrir shell local").clicked() {
                    app.connect_local();
                }
                if components::secondary_button(ui, "▣  Ir para workspace").clicked() {
                    app.screen = Screen::Workspace;
                }
            });
        },
    );
}

fn render_saved_connections(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Conexões salvas",
        "Perfis armazenados no vault criptografado.",
        |ui| {
            if app.connections.is_empty() {
                components::empty_state(
                    ui,
                    "Nenhuma conexão ainda",
                    "Crie seu primeiro perfil para começar.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    for profile in app.connections.clone() {
                        connection_row(app, ui, &profile);
                    }
                });
        },
    );
}

fn connection_row(
    app: &mut OpenPtlApp,
    ui: &mut egui::Ui,
    profile: &crate::libs::models::ConnectionProfile,
) {
    egui::Frame::new()
        .fill(theme::PANEL_RAISED)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 10))
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
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if components::secondary_button(ui, "Editar").clicked() {
                        app.start_editing(&profile.id);
                    }
                    if components::primary_button(ui, "Conectar").clicked() {
                        app.connect(&profile.id, SshConnectPurpose::Terminal, false);
                    }
                });
            });
        });
    ui.add_space(6.0);
}
