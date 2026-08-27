use eframe::egui::{self, RichText};

use crate::app::OpenPtlApp;

use super::{components, theme};

pub fn render(_app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            components::page_header(
                ui,
                "Produto",
                "Sobre o OpenPtl",
                "Um workspace remoto nativo, seguro e orientado à produtividade.",
            );
            ui.columns(2, |columns| {
                render_identity(&mut columns[0]);
                render_architecture(&mut columns[1]);
            });
        });
}

fn render_identity(ui: &mut egui::Ui) {
    components::card(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("OP").size(38.0).strong().color(theme::ACCENT));
            ui.add_space(8.0);
            ui.label(RichText::new("OpenPtl").size(24.0).strong());
            ui.label(RichText::new("secure remote workspace").color(theme::TEXT_MUTED));
            ui.add_space(18.0);
            ui.label(
                RichText::new("Cliente desktop nativo para conexões SSH e operações SFTP.")
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(18.0);
            components::badge(ui, "BUILD NATIVO", theme::SUCCESS);
        });
    });
}

fn render_architecture(ui: &mut egui::Ui) {
    components::section(ui, "Arquitetura", "Tecnologias usadas neste build.", |ui| {
        architecture_row(ui, "Interface", "Rust + eframe/egui", theme::ACCENT);
        architecture_row(ui, "Runtime", "Tokio + russh", theme::SUCCESS);
        architecture_row(
            ui,
            "Proteção",
            "Argon2id + XChaCha20-Poly1305",
            theme::WARNING,
        );
        architecture_row(
            ui,
            "Persistência",
            "Vault binário criptografado",
            theme::TEXT_MUTED,
        );
        components::divider(ui);
        ui.label(
            RichText::new("O aplicativo executa em um único binário e não depende de runtime web.")
                .small()
                .color(theme::TEXT_MUTED),
        );
    });
}

fn architecture_row(ui: &mut egui::Ui, title: &str, value: &str, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.vertical(|ui| {
            ui.label(RichText::new(title).small().color(theme::TEXT_MUTED));
            ui.label(RichText::new(value).strong());
        });
    });
    ui.add_space(9.0);
}
