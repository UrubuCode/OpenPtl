use eframe::egui::{self, Align2, RichText, Stroke, Vec2};

use crate::app::OpenPtlApp;

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    let Some(challenge) = app.host_challenge.clone() else {
        return;
    };
    let mut accept = false;
    let mut cancel = false;
    egui::Window::new("Verificação de segurança")
        .collapsible(false)
        .resizable(false)
        .fixed_size(Vec2::new(500.0, 0.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(theme::WARNING, RichText::new("!").size(24.0).strong());
                ui.vertical(|ui| {
                    ui.label(RichText::new("Host desconhecido").size(20.0).strong());
                    ui.label(RichText::new("Verifique a identidade do servidor antes de continuar.").color(theme::TEXT_MUTED));
                });
            });
            components::divider(ui);
            ui.label(RichText::new(format!("{}:{}", challenge.host, challenge.port)).strong());
            ui.label(RichText::new(format!("Tipo de chave: {}", challenge.key_type)).small().color(theme::TEXT_MUTED));
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .stroke(Stroke::new(1.0_f32, theme::WARNING.linear_multiply(0.65)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.label(RichText::new("Fingerprint SHA-256").small().color(theme::TEXT_MUTED));
                    ui.add_space(4.0);
                    ui.monospace(RichText::new(&challenge.fingerprint).color(theme::WARNING));
                });
            ui.add_space(10.0);
            ui.label(RichText::new("Aceite somente se esta impressão digital tiver sido confirmada por um canal confiável.").small().color(theme::TEXT_MUTED));
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if components::secondary_button(ui, "Cancelar").clicked() {
                    cancel = true;
                }
                if components::primary_button(ui, "Aceitar e conectar").clicked() {
                    accept = true;
                }
            });
        });
    if cancel {
        app.host_challenge = None;
    }
    if accept {
        app.host_challenge = None;
        app.connect(&challenge.profile_id, challenge.purpose, true);
    }
}
