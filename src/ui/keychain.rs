use eframe::egui::{self, Align, Layout, RichText};

use crate::app::OpenPtlApp;
use crate::libs::models::{KeychainEntry, KeychainEntryType};

use super::{components, theme};

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(24))
        .show(context, |ui| {
            components::page_header(
                ui,
                "Segredos",
                "Keychain",
                "Credenciais protegidas dentro do vault criptografado.",
            );
            ui.columns(2, |columns| {
                render_list(app, &mut columns[0]);
                render_new_entry(app, &mut columns[1]);
            });
        });
}

fn render_list(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Itens cadastrados",
        "Os valores secretos nunca são exibidos nesta lista.",
        |ui| {
            if app.keychain.is_empty() {
                components::empty_state(
                    ui,
                    "Nenhuma credencial",
                    "Adicione um segredo para reutilizá-lo nas conexões.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(500.0)
                .show(ui, |ui| {
                    for entry in app.keychain.clone() {
                        entry_card(app, ui, &entry);
                    }
                });
        },
    );
}

fn entry_card(app: &mut OpenPtlApp, ui: &mut egui::Ui, entry: &KeychainEntry) {
    egui::Frame::new()
        .fill(theme::PANEL_RAISED)
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(theme::ACCENT_DARK)
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new("•••").strong().color(theme::ACCENT));
                    });
                ui.vertical(|ui| {
                    ui.label(RichText::new(&entry.name).strong());
                    ui.label(
                        RichText::new(entry_type_label(&entry.entry_type))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if components::danger_button(ui, "Excluir").clicked() {
                        app.request_delete_keychain(&entry.id);
                    }
                });
            });
        });
    ui.add_space(8.0);
}

fn render_new_entry(app: &mut OpenPtlApp, ui: &mut egui::Ui) {
    components::section(
        ui,
        "Nova credencial",
        "Cadastre apenas o nome e o segredo necessário.",
        |ui| {
            components::field_label(ui, "Nome", "Ex.: chave de produção");
            ui.text_edit_singleline(&mut app.new_keychain_name);
            ui.add_space(10.0);
            components::field_label(
                ui,
                "Segredo ou senha",
                "O valor será armazenado criptografado.",
            );
            ui.add(egui::TextEdit::singleline(&mut app.new_keychain_secret).password(true));
            ui.add_space(16.0);
            if components::primary_button(ui, "Salvar credencial").clicked() {
                save_entry(app);
            }
        },
    );
}

fn save_entry(app: &mut OpenPtlApp) {
    let entry = KeychainEntry {
        id: String::new(),
        name: app.new_keychain_name.trim().to_string(),
        entry_type: KeychainEntryType::Secret,
        password: Some(app.new_keychain_secret.clone()),
        private_key: None,
        public_key: None,
        passphrase: None,
        created_at: 0,
    };
    if entry.name.is_empty() || app.new_keychain_secret.is_empty() {
        app.message = Some("Informe nome e segredo antes de salvar.".to_string());
        app.message_is_error = true;
        return;
    }
    let result = app
        .backend
        .as_mut()
        .map(|backend| backend.save_keychain(entry));
    match result {
        Some(Ok(_)) => {
            app.new_keychain_name.clear();
            app.new_keychain_secret.clear();
            app.refresh();
            app.message = Some("Credencial salva no vault.".to_string());
            app.message_is_error = false;
        }
        Some(Err(error)) => {
            app.message = Some(error.to_string());
            app.message_is_error = true;
        }
        None => {
            app.message = Some("Backend indisponível.".to_string());
            app.message_is_error = true;
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
