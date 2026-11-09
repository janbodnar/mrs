mod app;
mod text;
mod theme;

use std::path::PathBuf;

use app::EditorApp;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);

    let icon = eframe::egui::IconData {
        rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
        width: 64,
        height: 64,
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("mrs")
            .with_app_id("mrs")
            .with_icon(icon)
            .with_inner_size([1080.0, 760.0])
            .with_min_inner_size([460.0, 320.0]),
        ..Default::default()
    };

    eframe::run_native(
        "mrs",
        options,
        Box::new(move |cc| Ok(Box::new(EditorApp::new(cc, initial)))),
    )
}
