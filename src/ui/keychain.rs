use eframe::egui;

use crate::app::OpenPtlApp;
use crate::libs::models::{KeychainEntry, KeychainEntryType};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.heading("Keychain");
        ui.label("Credenciais armazenadas dentro do vault criptografado.");
        ui.add_space(16.0);

        ui.columns(2, |columns| {
            render_list(app, &mut columns[0]);
            render_new_entry(app, &mut columns[1]);
        });
    });
}

fn render_list(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading("Itens cadastrados");
    if app.keychain.is_empty() {
        ui.label(egui::RichText::new("Nenhuma credencial cadastrada.").weak());
        return;
    }
    for entry in app.keychain.clone() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&entry.name).strong());
                    ui.label(entry_type_label(&entry.entry_type));
                });
                if ui.small_button("Excluir").clicked() {
                    if let Some(backend) = &mut app.backend {
                        match backend.delete_keychain(&entry.id) {
                            Ok(()) => {
                                app.refresh();
                                app.message = Some("Credencial excluída.".to_string());
                                app.message_is_error = false;
                            }
                            Err(error) => {
                                app.message = Some(error.to_string());
                                app.message_is_error = true;
                            }
                        }
                    }
                }
            });
        });
    }
}

fn render_new_entry(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    ui.heading("Nova credencial");
    ui.label("Apenas o nome e um segredo são obrigatórios.");
    ui.add_space(8.0);
    ui.label("Nome");
    ui.text_edit_singleline(&mut app.new_keychain_name);
    ui.label("Segredo ou senha");
    ui.add(egui::TextEdit::singleline(&mut app.new_keychain_secret).password(true));
    if ui.button("Salvar credencial").clicked() {
        let entry = KeychainEntry {
            id: String::new(),
            name: app.new_keychain_name.clone(),
            entry_type: KeychainEntryType::Secret,
            password: Some(app.new_keychain_secret.clone()),
            private_key: None,
            public_key: None,
            passphrase: None,
            created_at: 0,
        };
        if let Some(backend) = &mut app.backend {
            match backend.save_keychain(entry) {
                Ok(_) => {
                    app.new_keychain_name.clear();
                    app.new_keychain_secret.clear();
                    app.refresh();
                    app.message = Some("Credencial salva.".to_string());
                    app.message_is_error = false;
                }
                Err(error) => {
                    app.message = Some(error.to_string());
                    app.message_is_error = true;
                }
            }
        }
    }
}

fn entry_type_label(entry_type: &KeychainEntryType) -> &'static str {
    match entry_type {
        KeychainEntryType::Password => "Senha",
        KeychainEntryType::SshKey => "Chave SSH",
        KeychainEntryType::Secret => "Segredo",
    }
}
