use eframe::egui::{self, Align2, RichText, Vec2};

use crate::app::{Dialog, OpenPtlApp};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    let Some(dialog) = app.dialog.clone() else {
        return;
    };

    let mut open = true;
    egui::Window::new("Confirmar ação")
        .collapsible(false)
        .resizable(false)
        .fixed_size(Vec2::new(420.0, 0.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut open)
        .show(context, |ui| {
            let (title, description, detail) = dialog_copy(app, &dialog);
            ui.label(RichText::new(title).size(20.0).strong());
            ui.add_space(8.0);
            ui.label(description);
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(RichText::new(detail).color(theme::TEXT_MUTED));
                });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if components::secondary_button(ui, "Cancelar").clicked() {
                    app.dialog = None;
                }
                if components::danger_button(ui, "Excluir definitivamente").clicked() {
                    app.confirm_dialog();
                }
            });
        });

    if !open {
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
