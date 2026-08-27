use eframe::egui::{self, Id, RichText, Stroke};

use crate::app::{Dialog, OpenPtlApp};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    let Some(dialog) = app.dialog.clone() else {
        return;
    };

    let modal_response = egui::Modal::new(Id::new("openptl_confirm_dialog"))
        .backdrop_color(egui::Color32::from_black_alpha(170))
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(20)),
        )
        .show(context, |ui| {
            ui.set_min_width(430.0);
            let (title, description, detail) = dialog_copy(app, &dialog);
            ui.label(RichText::new(title).size(20.0).strong());
            ui.add_space(8.0);
            ui.label(RichText::new(description).color(theme::TEXT_MUTED));
            ui.add_space(12.0);
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(11))
                .show(ui, |ui| {
                    ui.label(RichText::new(detail).color(theme::TEXT));
                });
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if components::secondary_button(ui, "Cancelar").clicked() {
                    app.dialog = None;
                }
                if components::danger_button(ui, "Excluir definitivamente").clicked() {
                    app.confirm_dialog();
                }
            });
        });

    if modal_response.backdrop_response.clicked()
        || context.input(|input| input.key_pressed(egui::Key::Escape))
    {
        app.dialog = None;
    }
}

fn dialog_copy(app: &OpenPtlApp, dialog: &Dialog) -> (&'static str, &'static str, String) {
    match dialog {
        Dialog::DeleteConnection(id) => {
            let name = app
                .connections
                .iter()
                .find(|profile| profile.id == *id)
                .map(|profile| profile.name.as_str())
                .unwrap_or("conexão selecionada");
            (
                "Excluir conexão?",
                "Esta ação remove o perfil do vault criptografado.",
                format!("Perfil: {name}"),
            )
        }
        Dialog::DeleteKeychain(id) => {
            let name = app
                .keychain
                .iter()
                .find(|entry| entry.id == *id)
                .map(|entry| entry.name.as_str())
                .unwrap_or("credencial selecionada");
            (
                "Excluir credencial?",
                "O segredo será removido do keychain protegido.",
                format!("Item: {name}"),
            )
        }
    }
}
