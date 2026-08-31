mod bridge;
mod dev_unlock;
mod editor_flow;
mod files_flow;
mod keychain_flow;
mod keymap;
mod known_hosts_flow;
mod mappers;
mod notes_flow;
mod session_flow;
mod settings_flow;
mod sync_flow;
mod terminal_view;
mod transfers_flow;
mod update_flow;
mod window_flow;
mod workspace_flow;

slint::include_modules!();

pub use bridge::run;
