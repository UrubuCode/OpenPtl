pub mod about;
pub mod challenges;
pub mod components;
pub mod connection_form;
pub mod connections;
pub mod dialogs;
pub mod home;
pub mod keychain;
pub mod layout;
pub mod settings;
pub mod theme;
pub mod vault_gate;
pub mod workspace;

use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    theme::apply(context);
    if app.startup_error.is_some() || app.status.locked {
        vault_gate::render(app, context);
        return;
    }

    layout::render(app, context);
    match app.screen {
        crate::app::Screen::Home => home::render(app, context),
        crate::app::Screen::Connections => connections::render(app, context),
        crate::app::Screen::Keychain => keychain::render(app, context),
        crate::app::Screen::Workspace => workspace::render(app, context),
        crate::app::Screen::Settings => settings::render(app, context),
        crate::app::Screen::About => about::render(app, context),
    }
    challenges::render(app, context);
    dialogs::render(app, context);
}
