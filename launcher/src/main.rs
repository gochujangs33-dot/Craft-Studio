#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod theme;
mod ui;
mod updater;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // Files passed on the command line (e.g. "Open with" / drag onto the exe)
    // are routed to the matching Craft app after the launcher appears.
    let file_args: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ArtCraft Studio")
        .with_maximized(true)
        .with_inner_size([1200.0, 750.0])
        .with_min_inner_size([720.0, 480.0])
        .with_drag_and_drop(true);

    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/app-256.png")) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        viewport = viewport.with_icon(egui::IconData { rgba: rgba.into_raw(), width, height });
    }

    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };

    eframe::run_native(
        "ArtCraft Studio",
        options,
        Box::new(move |cc| Ok(Box::new(ui::Launcher::new(cc, exe_dir, file_args)))),
    )
}
