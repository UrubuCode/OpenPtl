use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(_app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.heading(egui::RichText::new("OpenPtl").size(32.0).strong());
            ui.label("Cliente nativo para operações remotas seguras");
            ui.add_space(24.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(430.0);
                ui.label("Arquitetura");
                ui.label("Rust estável + eframe/egui + Tokio");
                ui.label("Vault criptografado com Argon2id e XChaCha20-Poly1305");
                ui.label("Sessões SSH/SFTP com russh");
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(
                        "Este build é integralmente nativo e executa em um único binário Rust.",
                    )
                    .strong(),
                );
            });
        });
    });
}
