use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke};

use crate::app::{key_mode_label, OpenPtlApp};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND))
        .show(context, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(72.0);
                ui.set_max_width(900.0);
                ui.columns(2, |columns| {
                    render_brand(&mut columns[0]);
                    render_auth_card(app, &mut columns[1]);
                });
            });
        });
}

fn render_brand(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        egui::Frame::new()
            .fill(theme::ACCENT_DARK)
            .stroke(Stroke::new(1.0_f32, theme::ACCENT))
            .corner_radius(egui::CornerRadius::same(18))
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.label(RichText::new("OP").size(34.0).strong().color(theme::TEXT));
            });
        ui.add_space(18.0);
        ui.label(RichText::new("OpenPtl").size(32.0).strong());
        ui.label(RichText::new("secure remote workspace").color(theme::TEXT_MUTED));
        ui.add_space(22.0);
        ui.label(RichText::new("Conecte-se aos seus ambientes remotos com uma experiência nativa, segura e organizada.").color(theme::TEXT_MUTED));
        ui.add_space(18.0);
        ui.horizontal_wrapped(|ui| {
            components::badge(ui, "SSH", theme::ACCENT);
            components::badge(ui, "SFTP", theme::SUCCESS);
            components::badge(ui, "VAULT LOCAL", theme::WARNING);
        });
    });
}

fn render_auth_card(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::card(ui, |ui| {
        ui.set_min_width(360.0);
        if let Some(error) = &app.startup_error {
            error_banner(ui, error);
            ui.add_space(12.0);
        }
        let initialized = app.status.initialized;
        ui.label(
            RichText::new(if initialized {
                "Bem-vindo de volta"
            } else {
                "Configure seu vault"
            })
            .size(21.0)
            .strong(),
        );
        ui.add_space(4.0);
        if initialized {
            ui.label(
                RichText::new("Desbloqueie o armazenamento protegido para continuar.")
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(16.0);
            components::badge(
                ui,
                key_mode_label(app.status.key_mode.as_ref()),
                theme::ACCENT,
            );
            ui.add_space(16.0);
            password_field(
                ui,
                "Senha mestre",
                "Sua senha não sai deste processo.",
                &mut app.password,
            );
            if components::primary_button(ui, "Desbloquear vault").clicked() {
                app.initialize_or_unlock();
            }
        } else {
            ui.label(
                RichText::new("Crie uma senha mestre para proteger suas credenciais locais.")
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(16.0);
            password_field(
                ui,
                "Nova senha mestre",
                "Use pelo menos 6 caracteres.",
                &mut app.password,
            );
            password_field(
                ui,
                "Confirmar senha",
                "Repita a senha mestre.",
                &mut app.password_confirmation,
            );
            if components::primary_button(ui, "Criar vault criptografado").clicked() {
                app.initialize_or_unlock();
            }
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.colored_label(theme::SUCCESS, "●");
            ui.label(
                RichText::new("Criptografia local ativa")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        });
    });
}

fn password_field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String) {
    components::field_label(ui, label, hint);
    ui.add(
        egui::TextEdit::singleline(value)
            .password(true)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(10.0);
}

fn error_banner(ui: &mut egui::Ui, message: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            theme::DANGER.r(),
            theme::DANGER.g(),
            theme::DANGER.b(),
            24,
        ))
        .stroke(Stroke::new(1.0_f32, theme::DANGER.linear_multiply(0.65)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.colored_label(theme::DANGER, "!");
                ui.label(RichText::new(message).small().color(theme::TEXT));
            });
        });
}
