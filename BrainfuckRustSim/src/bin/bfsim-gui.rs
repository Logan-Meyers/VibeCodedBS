//! `bfsim-gui [file.bf]`: the desktop IDE.

fn main() -> eframe::Result {
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    bfsim::gui::run(file)
}
