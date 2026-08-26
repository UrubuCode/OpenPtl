use eframe::egui;

use crate::libs::models::{ConnectionProfile, ConnectionProtocol};

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
        egui::Grid::new("connection_form_grid")
            .num_columns(2)
            .spacing([14.0, 10.0])
            .show(ui, |ui| {
                field(ui, "Nome", &mut self.name);
                field(ui, "Host", &mut self.host);
                field(ui, "Porta", &mut self.port);
                field(ui, "Usuário", &mut self.username);
                secret_field(ui, "Senha", &mut self.password);
                multiline_field(ui, "Chave privada", &mut self.private_key);
                field(ui, "Caminho remoto", &mut self.remote_path);
                ui.label("Protocolos");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.enable_ssh, "SSH");
                    ui.checkbox(&mut self.enable_sftp, "SFTP");
                });
                ui.end_row();
            });
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(310.0));
    ui.end_row();
}

fn secret_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(
        egui::TextEdit::singleline(value)
            .password(true)
            .desired_width(310.0),
    );
    ui.end_row();
}

fn multiline_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_rows(3)
            .desired_width(310.0),
    );
    ui.end_row();
}
