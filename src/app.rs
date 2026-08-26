use crate::backend::Backend;
use crate::libs::models::{
    AppSettings, ConnectionProfile, KeyMode, KeychainEntry, SshConnectPurpose, SshConnectResult,
    SshSessionInfo, VaultStatus,
};

use eframe::egui;

use crate::ui::connection_form::ConnectionForm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Connections,
    Keychain,
    Workspace,
    Settings,
    About,
}

impl Screen {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Visão geral",
            Self::Connections => "Conexões",
            Self::Keychain => "Keychain",
            Self::Workspace => "Workspace",
            Self::Settings => "Configurações",
            Self::About => "Sobre",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HostChallenge {
    pub profile_id: String,
    pub purpose: SshConnectPurpose,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

pub struct OpenPtlApp {
    pub backend: Option<Backend>,
    pub startup_error: Option<String>,
    pub status: VaultStatus,
    pub screen: Screen,
    pub password: String,
    pub password_confirmation: String,
    pub connections: Vec<ConnectionProfile>,
    pub connection_form: ConnectionForm,
    pub editing_connection_id: Option<String>,
    pub keychain: Vec<KeychainEntry>,
    pub new_keychain_name: String,
    pub new_keychain_secret: String,
    pub settings: AppSettings,
    pub settings_loaded: bool,
    pub selected_session: Option<String>,
    pub sessions: Vec<SshSessionInfo>,
    pub terminal_input: String,
    pub terminal_output: String,
    pub host_challenge: Option<HostChallenge>,
    pub message: Option<String>,
    pub message_is_error: bool,
}

impl OpenPtlApp {
    pub fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        match Backend::new() {
            Ok(backend) => {
                let status = backend.status().unwrap_or(VaultStatus {
                    initialized: false,
                    locked: true,
                    key_mode: None,
                    recoverable: false,
                });
                Self {
                    backend: Some(backend),
                    startup_error: None,
                    status,
                    screen: Screen::Home,
                    password: String::new(),
                    password_confirmation: String::new(),
                    connections: Vec::new(),
                    connection_form: ConnectionForm::default(),
                    editing_connection_id: None,
                    keychain: Vec::new(),
                    new_keychain_name: String::new(),
                    new_keychain_secret: String::new(),
                    settings: AppSettings::default(),
                    settings_loaded: false,
                    selected_session: None,
                    sessions: Vec::new(),
                    terminal_input: String::new(),
                    terminal_output: String::new(),
                    host_challenge: None,
                    message: None,
                    message_is_error: false,
                }
            }
            Err(error) => Self {
                backend: None,
                startup_error: Some(error.to_string()),
                status: VaultStatus {
                    initialized: false,
                    locked: true,
                    key_mode: None,
                    recoverable: false,
                },
                screen: Screen::Home,
                password: String::new(),
                password_confirmation: String::new(),
                connections: Vec::new(),
                connection_form: ConnectionForm::default(),
                editing_connection_id: None,
                keychain: Vec::new(),
                new_keychain_name: String::new(),
                new_keychain_secret: String::new(),
                settings: AppSettings::default(),
                settings_loaded: false,
                selected_session: None,
                sessions: Vec::new(),
                terminal_input: String::new(),
                terminal_output: String::new(),
                host_challenge: None,
                message: None,
                message_is_error: false,
            },
        }
    }

    pub fn refresh(&mut self) {
        let Some(backend) = &self.backend else {
            return;
        };
        if let Ok(status) = backend.status() {
            self.status = status;
        }
        if self.status.locked {
            return;
        }
        self.connections = backend.connections().unwrap_or_default();
        self.keychain = backend.keychain().unwrap_or_default();
        self.sessions = backend.sessions();
    }

    pub fn initialize_or_unlock(&mut self) {
        if self.password.trim().is_empty() {
            self.set_error("Informe uma senha mestre com pelo menos 6 caracteres.");
            return;
        }
        if !self.status.initialized && self.password != self.password_confirmation {
            self.set_error("A confirmação da senha não confere.");
            return;
        }
        let Some(backend) = &mut self.backend else {
            return;
        };
        let result = if self.status.initialized {
            backend.unlock(self.password.clone())
        } else {
            backend.initialize(self.password.clone())
        };
        match result {
            Ok(status) => {
                self.status = status;
                self.password.clear();
                self.password_confirmation.clear();
                self.set_message("Vault desbloqueado com segurança.");
                self.refresh();
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn lock(&mut self) {
        let Some(backend) = &mut self.backend else {
            return;
        };
        match backend.lock() {
            Ok(status) => {
                self.status = status;
                self.connections.clear();
                self.keychain.clear();
                self.settings_loaded = false;
                self.sessions.clear();
                self.selected_session = None;
                self.screen = Screen::Home;
                self.set_message("Vault bloqueado.");
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn save_connection(&mut self) {
        let profile = self
            .connection_form
            .to_profile(self.editing_connection_id.clone());
        let Some(backend) = &mut self.backend else {
            return;
        };
        match backend.save_connection(profile) {
            Ok(saved) => {
                self.editing_connection_id = Some(saved.id.clone());
                self.connection_form = ConnectionForm::from_profile(&saved);
                self.set_message("Conexão salva no vault criptografado.");
                self.refresh();
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn delete_connection(&mut self, id: &str) {
        let Some(backend) = &mut self.backend else {
            return;
        };
        match backend.delete_connection(id) {
            Ok(()) => {
                self.connection_form = ConnectionForm::default();
                self.editing_connection_id = None;
                self.set_message("Conexão excluída.");
                self.refresh();
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn start_editing(&mut self, id: &str) {
        if let Some(profile) = self.connections.iter().find(|item| item.id == id) {
            self.connection_form = ConnectionForm::from_profile(profile);
            self.editing_connection_id = Some(id.to_string());
            self.screen = Screen::Connections;
        }
    }

    pub fn connect(&mut self, profile_id: &str, purpose: SshConnectPurpose, accept_unknown: bool) {
        let Some(backend) = &mut self.backend else {
            return;
        };
        let challenge_purpose = purpose.clone();
        let result = if accept_unknown {
            backend.accept_and_connect(profile_id, purpose)
        } else {
            backend.connect(profile_id, purpose)
        };
        match result {
            Ok(SshConnectResult::Connected { session }) => {
                self.selected_session = Some(session.session_id);
                self.screen = Screen::Workspace;
                self.host_challenge = None;
                self.set_message("Sessão conectada.");
                self.refresh();
            }
            Ok(SshConnectResult::UnknownHostChallenge {
                host,
                port,
                key_type,
                fingerprint,
                ..
            }) => {
                self.host_challenge = Some(HostChallenge {
                    profile_id: profile_id.to_string(),
                    purpose: challenge_purpose,
                    host,
                    port,
                    key_type,
                    fingerprint,
                });
            }
            Ok(SshConnectResult::AuthRequired { message })
            | Ok(SshConnectResult::Error { message }) => self.set_error(message.message),
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn connect_local(&mut self) {
        let Some(backend) = &mut self.backend else {
            return;
        };
        match backend.connect_local(None) {
            Ok(session) => {
                self.selected_session = Some(session.session_id);
                self.screen = Screen::Workspace;
                self.refresh();
                self.set_message("Shell local conectado.");
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn send_terminal_input(&mut self) {
        let Some(session_id) = self.selected_session.clone() else {
            return;
        };
        let input = std::mem::take(&mut self.terminal_input);
        if input.is_empty() {
            return;
        }
        let Some(backend) = &mut self.backend else {
            return;
        };
        match backend.terminal_command(&session_id, &input) {
            Ok(output) => self.terminal_output.push_str(&output),
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub fn disconnect_selected(&mut self) {
        if let Some(session_id) = self.selected_session.take() {
            if let Some(backend) = &mut self.backend {
                backend.disconnect(&session_id);
            }
            self.refresh();
            self.set_message("Sessão encerrada.");
        }
    }

    fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
        self.message_is_error = false;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
        self.message_is_error = true;
    }
}

impl eframe::App for OpenPtlApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh();
        crate::ui::render(self, context);
        context.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

pub fn key_mode_label(mode: Option<&KeyMode>) -> &'static str {
    match mode {
        Some(KeyMode::Password) => "Senha mestre",
        Some(KeyMode::Keychain) => "Keychain do sistema",
        None => "Não configurado",
    }
}
