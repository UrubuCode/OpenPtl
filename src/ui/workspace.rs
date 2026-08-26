use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.heading("Workspace");
        ui.label("Sessões ativas e terminal integrado.");
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.button("Shell local").clicked() {
                app.connect_local();
            }
            if ui.button("Atualizar").clicked() {
                app.refresh();
            }
            if app.selected_session.is_some() && ui.button("Desconectar").clicked() {
                app.disconnect_selected();
            }
        });
        ui.separator();

        ui.columns(2, |columns| {
            render_sessions(app, &mut columns[0]);
            render_terminal(app, &mut columns[1]);
        });
    });
}

fn render_sessions(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading("Sessões");
    if app.sessions.is_empty() {
        ui.label(egui::RichText::new("Nenhuma sessão conectada.").weak());
        return;
    }
    for session in app.sessions.clone() {
        let selected = app.selected_session.as_deref() == Some(session.session_id.as_str());
        if ui
            .selectable_label(
                selected,
                format!(
                    "{} · {}",
                    session.session_kind,
                    short_id(&session.session_id)
                ),
            )
            .clicked()
        {
            app.selected_session = Some(session.session_id);
            app.terminal_output.clear();
        }
    }
}

fn render_terminal(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading("Terminal");
    if app.selected_session.is_none() {
        ui.label(egui::RichText::new("Selecione uma sessão para começar.").weak());
        return;
    }
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .max_height(420.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut app.terminal_output)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(20)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });
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
        if ui.button("Enviar").clicked() {
            app.send_terminal_input();
        }
    });
}

fn short_id(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}
