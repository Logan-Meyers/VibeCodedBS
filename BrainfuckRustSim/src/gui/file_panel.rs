//! Left sidebar: a tiny explorer listing `.bf` / `.b` files under a folder.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};

use super::theme;

pub struct FilePanel {
    pub root: Option<PathBuf>,
    entries: Vec<PathBuf>,
}

pub enum FileAction {
    Open(PathBuf),
    OpenFile,
    OpenFolder,
    New,
}

impl FilePanel {
    pub fn new(root: Option<PathBuf>) -> Self {
        let mut p = Self { root, entries: Vec::new() };
        p.refresh();
        p
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
        self.refresh();
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        if let Some(root) = &self.root {
            collect(root, 0, &mut self.entries);
            self.entries.sort();
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, current: Option<&Path>, dirty: bool) -> Option<FileAction> {
        let mut action = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::section_title(ui, "EXPLORER");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if ui.small_button("⟳").on_hover_text("refresh").clicked() {
                    self.refresh();
                }
                if ui.small_button("🗁").on_hover_text("open folder…").clicked() {
                    action = Some(FileAction::OpenFolder);
                }
                if ui.small_button("🗋").on_hover_text("open file…").clicked() {
                    action = Some(FileAction::OpenFile);
                }
                if ui.small_button("+").on_hover_text("new file").clicked() {
                    action = Some(FileAction::New);
                }
            });
        });
        ui.separator();

        let Some(root) = self.root.clone() else {
            ui.label(RichText::new("no folder open").color(theme::TEXT_FAINT));
            if ui.button("open folder…").clicked() {
                action = Some(FileAction::OpenFolder);
            }
            return action;
        };

        let name = root.file_name().map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned());
        ui.label(RichText::new(format!("▾ {}", name.to_uppercase())).color(theme::TEXT).strong().small())
            .on_hover_text(root.display().to_string());

        egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, false]).show(ui, |ui| {
            if self.entries.is_empty() {
                ui.label(RichText::new("  (no .bf files)").color(theme::TEXT_FAINT).small());
            }
            let mut last_dir: Option<PathBuf> = None;
            for path in &self.entries {
                let rel = path.strip_prefix(&root).unwrap_or(path);
                let depth = rel.components().count().saturating_sub(1);
                // Directory headers.
                let dir = rel.parent().map(Path::to_path_buf).filter(|d| !d.as_os_str().is_empty());
                if dir != last_dir {
                    if let Some(d) = &dir {
                        ui.label(RichText::new(format!("  {}▾ {}", "  ".repeat(depth - 1), d.file_name().unwrap_or_default().to_string_lossy())).color(theme::TEXT_DIM).small());
                    }
                    last_dir = dir;
                }
                let file = rel.file_name().unwrap_or_default().to_string_lossy();
                let is_current = current == Some(path.as_path());
                let label = format!("{}  {} {}{}", "  ".repeat(depth), "◆", file, if is_current && dirty { " ●" } else { "" });
                let text = if is_current { RichText::new(label).color(theme::ORANGE) } else { RichText::new(label).color(theme::TEXT) };
                let resp = ui.selectable_label(is_current, text);
                if resp.clicked() {
                    action = Some(FileAction::Open(path.clone()));
                }
            }
        });
        action
    }
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden || path.ends_with("target") {
            continue;
        }
        if path.is_dir() {
            collect(&path, depth + 1, out);
        } else if path.extension().is_some_and(|e| e == "bf" || e == "b") {
            out.push(path);
        }
    }
}
