use eframe::egui::{self, RichText};

use crate::libs::models::{ConnectionProfile, ConnectionProtocol};

use super::{components, theme};

#[derive(Clone)]
pub struct ConnectionForm {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub private_key: String,
    pub remote_path: String,
    pub enable_ssh: bool,
    pub enable_sftp: bool,
}

impl Default for ConnectionForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            password: String::new(),
            private_key: String::new(),
            remote_path: "/".to_string(),
            enable_ssh: true,
            enable_sftp: true,
        }
    }
}

impl ConnectionForm {
    pub fn from_profile(profile: &ConnectionProfile) -> Self {
        Self {
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone(),
            password: profile.password.clone().unwrap_or_default(),
            private_key: profile.private_key.clone().unwrap_or_default(),
            remote_path: profile
                .remote_path
                .clone()
                .unwrap_or_else(|| "/".to_string()),
            enable_ssh: profile.protocols.contains(&ConnectionProtocol::Ssh),
            enable_sftp: profile.protocols.contains(&ConnectionProtocol::Sftp),
        }
    }

    pub fn to_profile(&self, id: Option<String>) -> ConnectionProfile {
        let mut protocols = Vec::new();
        if self.enable_ssh {
            protocols.push(ConnectionProtocol::Ssh);
        }
        if self.enable_sftp {
            protocols.push(ConnectionProtocol::Sftp);
        }
        ConnectionProfile {
            id: id.unwrap_or_default(),
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port.parse().unwrap_or(22),
            username: self.username.clone(),
            password: (!self.password.trim().is_empty()).then(|| self.password.clone()),
            private_key: (!self.private_key.trim().is_empty()).then(|| self.private_key.clone()),
            keychain_id: None,
            remote_path: (!self.remote_path.trim().is_empty()).then(|| self.remote_path.clone()),
            protocols,
            kind: None,
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui) {
        components::card(ui, |ui| {
            ui.label(RichText::new("Identificação").strong());
            ui.label(
                RichText::new("Dê um nome ao perfil e informe o destino remoto.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);
            ui.columns(2, |columns| {
                text_field(
                    &mut columns[0],
                    "Nome do perfil",
                    "Ex.: Produção",
                    &mut self.name,
                );
                text_field(
                    &mut columns[1],
                    "Host",
                    "Ex.: server.example.com",
                    &mut self.host,
                );
            });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                text_field(
                    &mut columns[0],
                    "Usuário",
                    "Ex.: ubuntu",
                    &mut self.username,
                );
                text_field(&mut columns[1], "Porta", "22", &mut self.port);
            });
        });
        ui.add_space(10.0);
        components::card(ui, |ui| {
            ui.label(RichText::new("Autenticação e destino").strong());
            ui.label(
                RichText::new("Use senha, chave privada ou ambos como fallback.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);
            ui.columns(2, |columns| {
                secret_field(&mut columns[0], "Senha", &mut self.password);
                text_field(
                    &mut columns[1],
                    "Caminho remoto",
                    "Ex.: /home/ubuntu",
                    &mut self.remote_path,
                );
            });
            ui.add_space(8.0);
            components::field_label(
                ui,
                "Chave privada",
                "Cole o conteúdo PEM. O valor será protegido pelo vault.",
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.private_key)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY),
            );
        });
        ui.add_space(10.0);
        components::card(ui, |ui| {
            ui.label(RichText::new("Protocolos habilitados").strong());
            ui.label(
                RichText::new("Escolha quais recursos devem aparecer para este perfil.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                protocol_toggle(ui, "SSH", "Terminal interativo", &mut self.enable_ssh);
                protocol_toggle(ui, "SFTP", "Arquivos remotos", &mut self.enable_sftp);
            });
        });
    }
}

fn text_field(ui: &mut egui::Ui, label: &str, hint: &str, value: &mut String) {
    ui.vertical(|ui| {
        components::field_label(ui, label, hint);
        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
    });
}

fn secret_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.vertical(|ui| {
        components::field_label(ui, label, "Opcional");
        ui.add(
            egui::TextEdit::singleline(value)
                .password(true)
                .desired_width(f32::INFINITY),
        );
    });
}

fn protocol_toggle(ui: &mut egui::Ui, name: &str, description: &str, enabled: &mut bool) {
    egui::Frame::new()
        .fill(if *enabled {
            theme::ACCENT_DARK
        } else {
            theme::PANEL_RAISED
        })
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.checkbox(enabled, RichText::new(name).strong());
            ui.label(RichText::new(description).small().color(theme::TEXT_MUTED));
        });
}
