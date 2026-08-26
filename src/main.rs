#![allow(dead_code)]

mod app;
mod backend;
mod constants;
mod libs;
mod protocols;
mod ui;

use app::OpenPtlApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenPtl")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "OpenPtl",
        options,
        Box::new(|creation_context| Ok(Box::new(OpenPtlApp::new(creation_context)))),
    )
}
