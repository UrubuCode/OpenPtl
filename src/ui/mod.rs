pub mod about;
pub mod challenges;
pub mod connection_form;
pub mod connections;
pub mod home;
pub mod keychain;
pub mod layout;
pub mod settings;
pub mod vault_gate;
pub mod workspace;

use eframe::egui;

use crate::app::OpenPtlApp;

pub fn render(app: &mut OpenPtlApp, context: &egui::Context) {
    context.set_visuals(egui::Visuals::dark());
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
}
