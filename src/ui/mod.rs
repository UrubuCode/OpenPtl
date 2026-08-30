mod bridge;
mod keychain_flow;
mod keymap;
mod mappers;
mod session_flow;
mod terminal_view;

slint::include_modules!();

pub use bridge::run;
