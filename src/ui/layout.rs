use eframe::egui;

use crate::app::{OpenPtlApp, Screen};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::TopBottomPanel::top("header")
        .exact_height(58.0)
        .show(context, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(18.0);
                ui.heading(egui::RichText::new("OpenPtl").strong());
                ui.label(egui::RichText::new("cliente nativo").weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Bloquear vault").clicked() {
                        app.lock();
                    }
                    ui.label("Vault desbloqueado");
                });
            });
        });

    egui::SidePanel::left("navigation")
        .resizable(false)
        .default_width(214.0)
        .show(context, |ui| {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("NAVEGAÇÃO").small().strong().weak());
            ui.add_space(8.0);
            navigation_button(app, ui, Screen::Home);
            navigation_button(app, ui, Screen::Connections);
            navigation_button(app, ui, Screen::Keychain);
            navigation_button(app, ui, Screen::Workspace);
            navigation_button(app, ui, Screen::Settings);
            navigation_button(app, ui, Screen::About);
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.separator();
                ui.label(egui::RichText::new("Rust + egui").small().weak());
                ui.label(
                    egui::RichText::new("Armazenamento criptografado")
                        .small()
                        .weak(),
                );
            });
        });

    egui::TopBottomPanel::bottom("status")
        .exact_height(30.0)
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                if let Some(message) = &app.message {
                    let color = if app.message_is_error {
                        egui::Color32::from_rgb(245, 135, 135)
                    } else {
                        egui::Color32::from_rgb(140, 220, 165)
                    };
                    ui.colored_label(color, message);
                } else {
                    ui.label(egui::RichText::new("Pronto").weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} sessão(ões)", app.sessions.len()));
                });
            });
        });
}

fn navigation_button(app: &mut OpenPtlApp, ui: &mut egui::Ui, screen: Screen) {
    let selected = app.screen == screen;
    let text = egui::RichText::new(screen.label()).size(14.0);
    if ui.selectable_label(selected, text).clicked() {
        app.screen = screen;
    }
}
