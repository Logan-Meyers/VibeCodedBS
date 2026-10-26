//! The egui desktop app.

mod app;
pub mod editor;
mod file_panel;
pub mod memory_view;
mod sim;
pub mod theme;

pub use app::BfApp;

/// Launch the GUI, optionally opening `file`.
pub fn run(file: Option<std::path::PathBuf>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("bfsim")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native("bfsim", options, Box::new(move |cc| Ok(Box::new(BfApp::new(cc, file)))))
}
