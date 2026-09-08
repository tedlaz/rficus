#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;
mod config;
mod dnd;
mod job;
mod parse;
mod taskbar;
mod theme;
mod ui;
mod update;

fn main() -> eframe::Result {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icon.png")).ok();
    // No OS title bar: the app draws its own, including the window buttons.
    let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size([668.0, 760.0])
        .with_min_inner_size([668.0, 520.0])
        .with_decorations(false);
    viewport.icon = icon.map(std::sync::Arc::new);

    eframe::run_native(
        "rficus",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ui::App::new(&cc.egui_ctx)))),
    )
}
