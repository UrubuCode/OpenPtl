use eframe::egui::{self, Align, Layout, RichText, Stroke};

use crate::app::{OpenPtlApp, Screen};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    render_header(app, context);
    render_sidebar(app, context);
    render_status_bar(app, context);
}

fn render_header(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(20, 12)),
        )
        .show(context, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(theme::ACCENT_DARK)
                    .stroke(Stroke::new(1.0_f32, theme::ACCENT))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new("OP").strong().color(theme::TEXT));
                    });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("OpenPtl").size(18.0).strong());
                    ui.label(
                        RichText::new("secure remote workspace")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if components::secondary_button(ui, "Bloquear vault").clicked() {
                        app.lock();
                    }
                    components::badge(ui, "VAULT DESBLOQUEADO", theme::SUCCESS);
                });
            });
        });
}

fn render_sidebar(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::SidePanel::left("navigation")
        .resizable(false)
        .exact_width(236.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::same(14)),
        )
        .show(context, |ui| {
            ui.label(
                RichText::new("WORKSPACE")
                    .small()
                    .strong()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(8.0);
            for screen in [
                Screen::Home,
                Screen::Connections,
                Screen::Keychain,
                Screen::Workspace,
                Screen::Settings,
                Screen::About,
            ] {
                navigation_button(app, ui, screen);
            }
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                components::divider(ui);
                ui.label(
                    RichText::new("Rust + egui")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    RichText::new("Dados protegidos localmente")
                        .small()
                        .color(theme::TEXT_MUTED),
                );
            });
        });
}

fn render_status_bar(app: &OpenPtlApp, context: &egui::Context) {
    egui::TopBottomPanel::bottom("status")
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(16, 7)),
        )
        .show(context, |ui| {
            ui.horizontal(|ui| {
                let (label, color) = if app.message_is_error {
                    ("Atenção", theme::DANGER)
                } else if app.message.is_some() {
                    ("Pronto", theme::SUCCESS)
                } else {
                    ("Sistema", theme::TEXT_MUTED)
                };
                ui.colored_label(color, "●");
                ui.label(RichText::new(label).small().strong().color(color));
                if let Some(message) = &app.message {
                    ui.separator();
                    ui.label(RichText::new(message).small().color(theme::TEXT_MUTED));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} sessão(ões) ativas", app.sessions.len()))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
        });
}

fn navigation_button(app: &mut OpenPtlApp, ui: &mut egui::Ui, screen: Screen) {
    let selected = app.screen == screen;
    let fill = if selected {
        theme::ACCENT_DARK
    } else {
        theme::PANEL
    };
    let stroke = if selected {
        theme::ACCENT
    } else {
        theme::PANEL
    };
    let label = format!("{}   {}", screen_icon(screen), screen.label());
    if ui
        .add_sized(
            [204.0, 38.0],
            egui::Button::new(RichText::new(label).size(14.0).color(if selected {
                theme::TEXT
            } else {
                theme::TEXT_MUTED
            }))
            .fill(fill)
            .stroke(Stroke::new(1.0_f32, stroke))
            .corner_radius(egui::CornerRadius::same(8)),
        )
        .clicked()
    {
        app.screen = screen;
        if screen == Screen::Settings {
            app.settings_loaded = false;
        }
    }
    ui.add_space(5.0);
}

fn screen_icon(screen: Screen) -> &'static str {
    match screen {
        Screen::Home => "⌂",
        Screen::Connections => "▤",
        Screen::Keychain => "◆",
        Screen::Workspace => "▣",
        Screen::Settings => "⚙",
        Screen::About => "ⓘ",
    }
}
