use eframe::egui;

use crate::app::{key_mode_label, OpenPtlApp};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(90.0);
            ui.heading(egui::RichText::new("OpenPtl").size(34.0).strong());
            ui.label("Cliente nativo para conexões SSH e SFTP");
            ui.add_space(28.0);
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(26))
                .show(ui, |ui| {
                    ui.set_max_width(420.0);
                    if let Some(error) = &app.startup_error {
                        ui.colored_label(egui::Color32::from_rgb(245, 135, 135), error);
                        ui.add_space(12.0);
                    }
                    if app.status.initialized {
                        ui.heading("Desbloquear vault");
                        ui.label(format!(
                            "Modo de proteção: {}",
                            key_mode_label(app.status.key_mode.as_ref())
                        ));
                        ui.add_space(12.0);
                        password_field(ui, "Senha mestre", &mut app.password);
                        if ui.button("Desbloquear").clicked() {
                            app.initialize_or_unlock();
                        }
                    } else {
                        ui.heading("Configurar vault");
                        ui.label("Crie uma senha mestre para proteger as suas credenciais.");
                        ui.add_space(12.0);
                        password_field(ui, "Nova senha mestre", &mut app.password);
                        password_field(ui, "Confirmar senha", &mut app.password_confirmation);
                        if ui.button("Criar vault criptografado").clicked() {
                            app.initialize_or_unlock();
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "A senha nunca deixa o processo e não é armazenada em texto puro.",
                        )
                        .small()
                        .weak(),
                    );
                });
        });
    });
}

fn password_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(
        egui::TextEdit::singleline(value)
            .password(true)
            .desired_width(360.0),
    );
    ui.add_space(8.0);
}
