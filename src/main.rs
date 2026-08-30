#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod constants;
mod libs;
mod protocols;
mod ui;

fn main() -> anyhow::Result<()> {
    ui::run()
}
