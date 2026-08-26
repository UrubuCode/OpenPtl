use eframe::egui;

use crate::app::{OpenPtlApp, Screen};
use crate::libs::models::SshConnectPurpose;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Conexões");
            if ui.button("Nova").clicked() {
                app.connection_form = super::connection_form::ConnectionForm::default();
                app.editing_connection_id = None;
            }
        });
        ui.add_space(14.0);

        ui.columns(2, |columns| {
            render_list(app, &mut columns[0]);
            render_editor(app, &mut columns[1]);
        });
    });
}

fn render_list(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading("Perfis salvos");
    ui.add_space(6.0);
    if app.connections.is_empty() {
        ui.label(egui::RichText::new("Cadastre a primeira conexão ao lado.").weak());
        return;
    }
    for profile in app.connections.clone() {
        let selected = app.editing_connection_id.as_deref() == Some(profile.id.as_str());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.editing_connection_id, Some(profile.id.clone()), "");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&profile.name).strong());
                    ui.label(format!(
                        "{}@{}:{}",
                        profile.username, profile.host, profile.port
                    ));
                    ui.label(
                        egui::RichText::new(protocol_labels(&profile))
                            .small()
                            .weak(),
                    );
                });
            });
            ui.horizontal(|ui| {
                if ui.small_button("Editar").clicked() {
                    app.start_editing(&profile.id);
                }
                if profile
                    .protocols
                    .iter()
                    .any(|item| matches!(item, crate::libs::models::ConnectionProtocol::Ssh))
                    && ui.small_button("SSH").clicked()
                {
                    app.connect(&profile.id, SshConnectPurpose::Terminal, false);
                }
                if profile
                    .protocols
                    .iter()
                    .any(|item| matches!(item, crate::libs::models::ConnectionProtocol::Sftp))
                    && ui.small_button("SFTP").clicked()
                {
                    app.connect(&profile.id, SshConnectPurpose::Sftp, false);
                }
            });
        });
        if selected {
            ui.add_space(4.0);
        }
    }
}

fn render_editor(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading(if app.editing_connection_id.is_some() {
        "Editar conexão"
    } else {
        "Nova conexão"
    });
    ui.add_space(6.0);
    app.connection_form.render(ui);
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui.button("Salvar conexão").clicked() {
            app.save_connection();
        }
        if app.editing_connection_id.is_some() && ui.button("Excluir").clicked() {
            if let Some(id) = app.editing_connection_id.clone() {
                app.delete_connection(&id);
            }
        }
        if ui.button("Abrir workspace").clicked() {
            app.screen = Screen::Workspace;
        }
    });
}

fn protocol_labels(profile: &crate::libs::models::ConnectionProfile) -> String {
    profile
        .protocols
        .iter()
        .map(|protocol| match protocol {
            crate::libs::models::ConnectionProtocol::Ssh => "SSH",
            crate::libs::models::ConnectionProtocol::Sftp => "SFTP",
            _ => "legado",
        })
        .collect::<Vec<_>>()
        .join(" · ")
}
