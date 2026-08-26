use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    let Some(challenge) = app.host_challenge.clone() else {
        return;
    };
    let mut accept = false;
    let mut cancel = false;
    egui::Window::new("Host desconhecido")
        .collapsible(false)
        .resizable(false)
        .show(context, |ui| {
            ui.label(format!("O host {}:{} não está no known_hosts.", challenge.host, challenge.port));
            ui.label(format!("Tipo de chave: {}", challenge.key_type));
            ui.monospace(&challenge.fingerprint);
            ui.add_space(8.0);
            ui.label("Aceite apenas se a impressão digital tiver sido verificada por um canal confiável.");
            ui.horizontal(|ui| {
                if ui.button("Aceitar e conectar").clicked() {
                    accept = true;
                }
                if ui.button("Cancelar").clicked() {
                    cancel = true;
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
