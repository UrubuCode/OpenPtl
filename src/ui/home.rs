use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.heading("Visão geral");
        ui.label("Gerencie conexões seguras em uma aplicação Rust nativa.");
        ui.add_space(22.0);

        ui.horizontal(|ui| {
            metric_card(
                ui,
                "Conexões",
                app.connections.len().to_string(),
                "Perfis protegidos",
            );
            metric_card(
                ui,
                "Sessões",
                app.sessions.len().to_string(),
                "Terminais ativos",
            );
            metric_card(
                ui,
                "Credenciais",
                app.keychain.len().to_string(),
                "Itens no keychain",
            );
        });
        ui.add_space(24.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.heading("Ações rápidas");
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Nova conexão").clicked() {
                    app.connection_form = crate::ui::connection_form::ConnectionForm::default();
                    app.editing_connection_id = None;
                    app.screen = crate::app::Screen::Connections;
                }
                if ui.button("Abrir shell local").clicked() {
                    app.connect_local();
                }
                if ui.button("Ir para workspace").clicked() {
                    app.screen = crate::app::Screen::Workspace;
                }
            });
        });
        ui.add_space(20.0);

        ui.heading("Conexões salvas");
        if app.connections.is_empty() {
            ui.label(egui::RichText::new("Nenhuma conexão cadastrada ainda.").weak());
        } else {
            for profile in app.connections.clone() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&profile.name).strong());
                    ui.label(format!(
                        "{}@{}:{}",
                        profile.username, profile.host, profile.port
                    ));
                    if ui.small_button("Conectar").clicked() {
                        app.connect(
                            &profile.id,
                            crate::libs::models::SshConnectPurpose::Terminal,
                            false,
                        );
                    }
                    if ui.small_button("Editar").clicked() {
                        app.start_editing(&profile.id);
                    }
                });
            }
        }
    });
}

fn metric_card(ui: &mut egui::Ui, title: &str, value: String, description: &str) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(180.0);
        ui.label(egui::RichText::new(title).weak());
        ui.heading(value);
        ui.label(egui::RichText::new(description).small().weak());
    });
}
