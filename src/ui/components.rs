use eframe::egui::{self, Color32, RichText, Stroke};

use super::theme;

pub fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(eyebrow.to_uppercase())
                    .small()
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(2.0);
            ui.heading(RichText::new(title).size(26.0).strong());
            ui.label(RichText::new(description).color(theme::TEXT_MUTED));
        });
    });
    ui.add_space(18.0);
}

pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
        .show(ui, add_contents)
        .inner
}

pub fn section<R>(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    card(ui, |ui| {
        ui.label(RichText::new(title).size(16.0).strong());
        if !description.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(description).small().color(theme::TEXT_MUTED));
        }
        ui.add_space(12.0);
        add_contents(ui)
    })
}

pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).strong())
            .fill(theme::ACCENT_DARK)
            .stroke(Stroke::new(1.0_f32, theme::ACCENT)),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(label).fill(theme::PANEL_RAISED))
}

pub fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::DANGER))
            .fill(theme::PANEL_RAISED)
            .stroke(Stroke::new(1.0_f32, theme::DANGER)),
    )
}

pub fn badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.linear_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color.linear_multiply(0.55)))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(label).small().strong().color(color));
        });
}

pub fn empty_state(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(22.0);
        ui.label(RichText::new(title).size(16.0).strong());
        ui.add_space(4.0);
        ui.label(RichText::new(description).color(theme::TEXT_MUTED));
        ui.add_space(22.0);
    });
}

pub fn field_label(ui: &mut egui::Ui, label: &str, hint: &str) {
    ui.label(RichText::new(label).strong());
    if !hint.is_empty() {
        ui.label(RichText::new(hint).small().color(theme::TEXT_MUTED));
    }
}

pub fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}
