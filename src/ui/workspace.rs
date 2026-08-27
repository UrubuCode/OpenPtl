use eframe::egui::{self, Align, Layout, RichText, Stroke};

use crate::app::OpenPtlApp;

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            components::page_header(
                ui,
                "Operações",
                "Workspace",
                "Acompanhe sessões remotas e shells locais em tempo real.",
            );
            render_toolbar(app, ui);
            ui.add_space(14.0);
            ui.columns(2, |columns| {
                render_sessions(app, &mut columns[0]);
                render_terminal(app, &mut columns[1]);
            });
        });
}

fn render_toolbar(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::card(ui, |ui| {
        ui.horizontal(|ui| {
            if components::primary_button(ui, "＋  Shell local").clicked() {
                app.connect_local();
            }
            if components::secondary_button(ui, "↻  Atualizar").clicked() {
                app.refresh();
            }
            if app.selected_session.is_some()
                && components::danger_button(ui, "Desconectar").clicked()
            {
                app.disconnect_selected();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if app.sessions.is_empty() {
                    "Nenhuma sessão"
                } else {
                    "Sessões disponíveis"
                };
                components::badge(
                    ui,
                    label,
                    if app.sessions.is_empty() {
                        theme::TEXT_MUTED
                    } else {
                        theme::SUCCESS
                    },
                );
            });
        });
    });
}

fn render_sessions(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(ui, "Sessões", "Conexões abertas nesta execução.", |ui| {
        if app.sessions.is_empty() {
            components::empty_state(
                ui,
                "Workspace vazio",
                "Abra um shell local ou conecte um perfil SSH.",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(500.0)
            .show(ui, |ui| {
                for session in app.sessions.clone() {
                    let selected =
                        app.selected_session.as_deref() == Some(session.session_id.as_str());
                    let fill = if selected {
                        theme::ACCENT_DARK
                    } else {
                        theme::PANEL_RAISED
                    };
                    let border = if selected {
                        theme::ACCENT
                    } else {
                        theme::BORDER
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(Stroke::new(1.0_f32, border))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(11))
                        .show(ui, |ui| {
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(format!(
                                        "{}  ·  {}",
                                        session.session_kind,
                                        short_id(&session.session_id)
                                    ))
                                    .strong(),
                                )
                                .clicked()
                            {
                                app.selected_session = Some(session.session_id.clone());
                                app.terminal_output.clear();
                            }
                        });
                    ui.add_space(7.0);
                }
            });
    });
}

fn render_terminal(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Terminal",
        "A entrada é enviada para a sessão selecionada.",
        |ui| {
            if app.selected_session.is_none() {
                components::empty_state(
                    ui,
                    "Nenhuma sessão selecionada",
                    "Selecione uma sessão ao lado para abrir o terminal.",
                );
                return;
            }
            egui::Frame::new()
                .fill(theme::BACKGROUND)
                .stroke(Stroke::new(1.0_f32, theme::BORDER))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(395.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut app.terminal_output)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(18)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.terminal_input)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .hint_text("Digite um comando ou entrada do shell"),
                );
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    app.send_terminal_input();
                }
                if components::primary_button(ui, "Enviar").clicked() {
                    app.send_terminal_input();
                }
            });
        },
    );
}

fn short_id(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}
