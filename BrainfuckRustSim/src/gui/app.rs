//! Top-level app: layout of the three panes, menus, shortcuts, status bar.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};

use super::editor::{self, Document, EditorContext};
use super::file_panel::{FileAction, FilePanel};
use super::memory_view::{MemoryView, Mode, tab};
use super::sim::{RunState, Sim, accent_button, human};
use super::theme;
use crate::core::{self, Diagnostic, FormatOptions, Indent, Severity};

const WELCOME: &str = "\
welcome to bfsim  -[>+<]-
every character that isn't one of +-<>.,[] is a comment
lines starting with # are comments too; # BREAKPOINT pauses here

prints \"Hi!\" then a newline
++++++++[>+++++++++<-]>   cell 1 = 72 ('H')
.
+++++++++++++++++++++++++++++++++ .   'i' = 105
# BREAKPOINT
[-]+++++++++++++++++++++++++++++++++ .   '!' = 33
[-]++++++++++ .                           newline
";

enum Pending {
    Open(PathBuf),
    New,
}

pub struct BfApp {
    doc: Document,
    files: FilePanel,
    sim: Sim,
    mem: MemoryView,
    diagnostics: Vec<Diagnostic>,
    diag_version: u64,
    font_size: f32,
    indent: Indent,
    status: Option<(String, Instant)>,
    cursor: (usize, usize),
    pending: Option<Pending>,
    jump_to: Option<usize>,
    show_settings: bool,
    title_cache: String,
    capture: Option<Capture>,
}

/// Headless-ish screenshot hook for docs and smoke tests:
/// `BFSIM_SCREENSHOT=out.rgba` saves the window (as `w h\n` + RGBA bytes)
/// once the program stops, then quits. `BFSIM_AUTORUN=1` presses Run,
/// `BFSIM_SPEED=<log10 ops/s>` sets the speed, `BFSIM_MEM=heat|usage` picks
/// the memory view.
struct Capture {
    path: PathBuf,
    frame: u32,
    requested: bool,
    autorun: bool,
}

impl BfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let root = file
            .as_ref()
            .and_then(|f| f.parent().map(|p| p.to_path_buf()))
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| {
                let ex = PathBuf::from("examples");
                if ex.is_dir() { std::fs::canonicalize(ex).ok() } else { std::env::current_dir().ok() }
            });
        let mut app = Self {
            doc: Document::new(None, WELCOME.to_owned()),
            files: FilePanel::new(root),
            sim: Sim::default(),
            mem: MemoryView::default(),
            diagnostics: Vec::new(),
            diag_version: u64::MAX,
            font_size: 14.0,
            indent: Indent::Spaces(4),
            status: None,
            cursor: (0, 0),
            pending: None,
            jump_to: None,
            show_settings: false,
            title_cache: String::new(),
            capture: std::env::var_os("BFSIM_SCREENSHOT").map(|p| Capture {
                path: PathBuf::from(p),
                frame: 0,
                requested: false,
                autorun: std::env::var_os("BFSIM_AUTORUN").is_some(),
            }),
        };
        if let Some(speed) = std::env::var("BFSIM_SPEED").ok().and_then(|s| s.parse().ok()) {
            app.sim.speed_log = speed;
        }
        match std::env::var("BFSIM_MEM").as_deref() {
            Ok("heat") => app.mem.mode = Mode::Heat,
            Ok("usage") => app.mem.mode = Mode::Usage,
            _ => {}
        }
        if let Some(f) = file {
            app.open(std::fs::canonicalize(&f).unwrap_or(f));
        }
        app
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    fn open(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.doc = Document::new(Some(path.clone()), text);
                self.sim.reset();
                self.flash(format!("opened {}", self.doc.title()));
            }
            Err(e) => self.flash(format!("✖ couldn't open {}: {e}", path.display())),
        }
    }

    /// Run `p` now, or ask first if there are unsaved changes.
    fn guarded(&mut self, p: Pending) {
        if self.doc.is_dirty() {
            self.pending = Some(p);
        } else {
            self.do_pending(p);
        }
    }

    fn do_pending(&mut self, p: Pending) {
        match p {
            Pending::Open(path) => self.open(path),
            Pending::New => {
                self.doc = Document::new(None, String::new());
                self.sim.reset();
            }
        }
    }

    fn save(&mut self, save_as: bool) {
        let path = match (&self.doc.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => {
                let mut dlg = rfd::FileDialog::new().add_filter("Brainfuck", &["bf", "b"]).set_file_name(self.doc.title());
                if let Some(root) = &self.files.root {
                    dlg = dlg.set_directory(root);
                }
                dlg.save_file()
            }
        };
        let Some(path) = path else { return };
        match std::fs::write(&path, &self.doc.text) {
            Ok(()) => {
                self.doc.path = Some(path.clone());
                self.doc.mark_saved();
                self.files.refresh();
                self.flash(format!("saved {}", self.doc.title()));
            }
            Err(e) => self.flash(format!("✖ save failed: {e}")),
        }
    }

    fn handle_file_action(&mut self, a: FileAction) {
        match a {
            FileAction::Open(p) => self.guarded(Pending::Open(p)),
            FileAction::New => self.guarded(Pending::New),
            FileAction::OpenFile => {
                let mut dlg = rfd::FileDialog::new().add_filter("Brainfuck", &["bf", "b"]).add_filter("Any", &["*"]);
                if let Some(root) = &self.files.root {
                    dlg = dlg.set_directory(root);
                }
                if let Some(p) = dlg.pick_file() {
                    if let Some(parent) = p.parent() {
                        self.files.set_root(parent.to_path_buf());
                    }
                    self.guarded(Pending::Open(p));
                }
            }
            FileAction::OpenFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.files.set_root(dir);
                }
            }
        }
    }

    fn format(&mut self) {
        let opts = FormatOptions { indent: self.indent, hash_comments: self.sim.hash_comments, ..Default::default() };
        if editor::format_document(&mut self.doc, opts) {
            self.flash("formatted ✔");
        } else {
            self.flash("already formatted");
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m, k| KeyboardShortcut::new(m, k);
        let hit = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if hit(sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.save(true);
        } else if hit(sc(Modifiers::COMMAND, Key::S)) {
            self.save(false);
        }
        if hit(sc(Modifiers::COMMAND, Key::O)) {
            self.handle_file_action(FileAction::OpenFile);
        }
        if hit(sc(Modifiers::COMMAND, Key::N)) {
            self.handle_file_action(FileAction::New);
        }
        if hit(sc(Modifiers::COMMAND | Modifiers::SHIFT, Key::F)) || hit(sc(Modifiers::ALT | Modifiers::SHIFT, Key::F)) {
            self.format();
        }
        if hit(sc(Modifiers::SHIFT, Key::F5)) {
            self.sim.reset();
        } else if hit(sc(Modifiers::NONE, Key::F5)) {
            self.sim.run(&self.doc);
        }
        if hit(sc(Modifiers::NONE, Key::F6)) {
            self.sim.pause();
        }
        if hit(sc(Modifiers::NONE, Key::F10)) {
            self.sim.step(&self.doc);
        }
        if hit(sc(Modifiers::COMMAND, Key::Equals)) || hit(sc(Modifiers::COMMAND, Key::Plus)) {
            self.font_size = (self.font_size + 1.0).min(28.0);
        }
        if hit(sc(Modifiers::COMMAND, Key::Minus)) {
            self.font_size = (self.font_size - 1.0).max(9.0);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("-[>+<]-").color(theme::ORANGE).strong());
            ui.menu_button("File", |ui| {
                if ui.button("New            ⌘N").clicked() {
                    self.handle_file_action(FileAction::New);
                }
                if ui.button("Open File…     ⌘O").clicked() {
                    self.handle_file_action(FileAction::OpenFile);
                }
                if ui.button("Open Folder…").clicked() {
                    self.handle_file_action(FileAction::OpenFolder);
                }
                ui.separator();
                if ui.button("Save           ⌘S").clicked() {
                    self.save(false);
                }
                if ui.button("Save As…      ⇧⌘S").clicked() {
                    self.save(true);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Format Document  ⇧⌘F").clicked() {
                    self.format();
                }
                ui.separator();
                ui.label(RichText::new("indent with").color(theme::TEXT_DIM));
                ui.radio_value(&mut self.indent, Indent::Spaces(2), "2 spaces");
                ui.radio_value(&mut self.indent, Indent::Spaces(4), "4 spaces");
                ui.radio_value(&mut self.indent, Indent::Tab, "tabs");
            });
            ui.menu_button("Run", |ui| {
                if ui.button("Run / Continue   F5").clicked() {
                    self.sim.run(&self.doc);
                }
                if ui.button("Pause            F6").clicked() {
                    self.sim.pause();
                }
                if ui.button("Step             F10").clicked() {
                    self.sim.step(&self.doc);
                }
                if ui.button("Reset         ⇧F5").clicked() {
                    self.sim.reset();
                }
                ui.separator();
                if ui.button("Clear gutter breakpoints").clicked() {
                    self.doc.gutter_breaks.clear();
                    self.sim.sync_breakpoints(&self.doc);
                }
            });
            ui.menu_button("View", |ui| {
                ui.radio_value(&mut self.mem.mode, Mode::Numeric, "Memory: numeric");
                ui.radio_value(&mut self.mem.mode, Mode::Heat, "Memory: heat map");
                ui.radio_value(&mut self.mem.mode, Mode::Usage, "Memory: usage bars");
                ui.separator();
                if ui.button("Zoom in    ⌘=").clicked() {
                    self.font_size = (self.font_size + 1.0).min(28.0);
                }
                if ui.button("Zoom out   ⌘-").clicked() {
                    self.font_size = (self.font_size - 1.0).max(9.0);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let white = egui::Color32::WHITE;
            let (state, _) = self.sim.state.label();
            ui.label(RichText::new(format!(" -[>+<]-  {state}")).color(white).strong());
            if let Some(vm) = &self.sim.vm {
                ui.label(RichText::new(format!("pc {}/{}", vm.pc, vm.program.len())).color(white));
                ui.label(RichText::new(format!("ptr 0x{:04X}", vm.ptr)).color(white));
                ui.label(RichText::new(format!("*ptr {}", vm.current_cell())).color(white));
                ui.label(RichText::new(format!("ops {}", human(vm.steps as f64))).color(white));
                if self.sim.is_running() {
                    ui.label(RichText::new(format!("{} op/s", human(self.sim.ops_per_sec))).color(white));
                }
            }
            if let Some((msg, t)) = &self.status
                && t.elapsed() < Duration::from_secs(4)
            {
                let short: String = msg.chars().take(48).collect();
                ui.label(RichText::new(format!("│ {short}")).color(theme::AMBER));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bits = self.sim.config.cell.bits();
                ui.label(RichText::new(format!("{bits}-bit cells  ")).color(white));
                ui.label(RichText::new("Brainfuck").color(white));
                ui.label(RichText::new(match self.indent {
                    Indent::Tab => "Tabs".to_owned(),
                    Indent::Spaces(n) => format!("Spaces: {n}"),
                }).color(white));
                ui.label(RichText::new(format!("Ln {}, Col {}", self.cursor.0 + 1, self.cursor.1 + 1)).color(white));
                let (e, w) = (
                    self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count(),
                    self.diagnostics.iter().filter(|d| d.severity == Severity::Warning).count(),
                );
                ui.label(RichText::new(format!("✖ {e}  ⚠ {w}")).color(white));
            });
        });
    }

    fn editor_pane(&mut self, ui: &mut egui::Ui) {
        // Tab strip.
        ui.horizontal(|ui| {
            let dirty = if self.doc.is_dirty() { " ●" } else { "" };
            tab(ui, true, &format!("◆ {}{dirty}", self.doc.title()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("{ } format").on_hover_text("Format document (⇧⌘F)").clicked() {
                    self.format();
                }
            });
        });
        if let Some(p) = &self.doc.path {
            let crumbs = p.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev()
                .map(|c| c.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" › ");
            ui.label(RichText::new(crumbs).color(theme::TEXT_FAINT).small());
        }
        ui.separator();

        // Problems panel along the bottom of the editor.
        egui::Panel::bottom("problems").resizable(true).default_size(110.0).min_size(48.0).show(ui, |ui| {
            ui.add_space(3.0);
            let n = self.diagnostics.len();
            ui.horizontal(|ui| {
                theme::section_title(ui, "PROBLEMS");
                ui.label(RichText::new(n.to_string()).color(if n == 0 { theme::TEXT_FAINT } else { theme::ORANGE }).small());
            });
            egui::ScrollArea::vertical().id_salt("problems").auto_shrink([false, false]).show(ui, |ui| {
                if n == 0 {
                    ui.label(RichText::new("✔ no problems: brackets balanced, vibes immaculate").color(theme::OK).small());
                }
                for d in &self.diagnostics {
                    let c = editor::severity_color(d.severity);
                    let text = RichText::new(format!("{:<7} Ln {:>3}, Col {:>3}  {}", d.severity.label(), d.line + 1, d.col + 1, d.message)).color(c).small();
                    if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text("jump to location").clicked() {
                        self.jump_to = Some(d.span.start);
                    }
                }
            });
        });

        let (current_span, current_line) = match &self.sim.vm {
            Some(vm) if !self.sim.is_running() || self.sim.speed_log < 3.5 => {
                (vm.program.spans.get(vm.pc).cloned(), vm.current_line())
            }
            _ => (None, None),
        };
        let gutter_before = self.doc.gutter_breaks.clone();
        let out = editor::show(
            ui,
            &mut self.doc,
            EditorContext {
                diagnostics: &self.diagnostics,
                current_span,
                current_line,
                hash_comments: self.sim.hash_comments,
                font_size: self.font_size,
                indent: self.indent,
                jump_to: self.jump_to.take(),
            },
        );
        self.cursor = (out.cursor_line, out.cursor_col);
        if out.changed && self.sim.invalidate() {
            self.flash("code changed: simulation reset");
        }
        if self.doc.gutter_breaks != gutter_before {
            self.sim.sync_breakpoints(&self.doc);
        }
    }

    fn sim_pane(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            theme::section_title(ui, "SIMULATOR");
            let (label, color) = self.sim.state.label();
            ui.label(RichText::new(label).color(color).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(self.show_settings, "⚙").on_hover_text("machine settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });
        if let RunState::Paused(why) = &self.sim.state {
            ui.label(RichText::new(format!("  {why}")).color(theme::AMBER).small());
        }
        self.sim.controls(ui, &self.doc);
        if self.show_settings {
            egui::Frame::new().fill(theme::BG).inner_margin(6.0).corner_radius(2.0).show(ui, |ui| {
                self.sim.settings(ui);
            });
        }
        ui.separator();

        egui::Panel::bottom("console").resizable(true).default_size(200.0).min_size(80.0).show(ui, |ui| {
            ui.add_space(4.0);
            theme::section_title(ui, "CONSOLE");
            self.sim.console(ui);
        });

        theme::section_title(ui, "MEMORY");
        self.mem.toolbar(ui);
        ui.add_space(4.0);
        self.mem.show(ui, self.sim.vm.as_ref());
    }

    fn unsaved_dialog(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut choice = None;
        egui::Window::new(RichText::new("unsaved changes").color(theme::ORANGE))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("{} has unsaved changes.", self.doc.title()));
                ui.horizontal(|ui| {
                    if ui.add(accent_button("save")).clicked() {
                        choice = Some(0);
                    }
                    if ui.button("discard").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
        match choice {
            Some(0) => {
                self.save(false);
                if !self.doc.is_dirty()
                    && let Some(p) = self.pending.take()
                {
                    self.do_pending(p);
                }
            }
            Some(1) => {
                if let Some(p) = self.pending.take() {
                    self.do_pending(p);
                }
            }
            Some(2) => self.pending = None,
            _ => {}
        }
    }
}

impl BfApp {
    fn capture_tick(&mut self, ctx: &egui::Context) {
        let Some(cap) = &mut self.capture else { return };
        cap.frame += 1;
        if cap.autorun && cap.frame == 3 {
            self.sim.run(&self.doc);
        }
        let settled = cap.frame > 10 && !self.sim.is_running();
        if !cap.requested && (settled || cap.frame > 900) {
            cap.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let mut bytes = format!("{} {}\n", img.size[0], img.size[1]).into_bytes();
            bytes.extend(img.pixels.iter().flat_map(|c| c.to_array()));
            if let Err(e) = std::fs::write(&cap.path, bytes) {
                eprintln!("screenshot failed: {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

impl eframe::App for BfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.sim.tick(&ctx);

        if self.diag_version != self.doc.version {
            let opts = core::ParseOptions { hash_comments: self.sim.hash_comments, ..Default::default() };
            self.diagnostics = core::check(&self.doc.text, opts);
            self.diag_version = self.doc.version;
        }

        let title = format!("{}{} — bfsim", self.doc.title(), if self.doc.is_dirty() { " ●" } else { "" });
        if title != self.title_cache {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_cache = title;
        }

        egui::Panel::top("menu").frame(egui::Frame::new().fill(theme::BG_BAR).inner_margin(egui::Margin::symmetric(6, 3))).show(ui, |ui| {
            self.menu_bar(ui);
        });
        egui::Panel::bottom("status")
            .exact_size(22.0)
            .frame(egui::Frame::new().fill(theme::DEEP_RED))
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("files")
            .resizable(true)
            .default_size(210.0)
            .size_range(140.0..=380.0)
            .frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(6.0))
            .show(ui, |ui| {
                let current = self.doc.path.clone();
                if let Some(a) = self.files.show(ui, current.as_deref(), self.doc.is_dirty()) {
                    self.handle_file_action(a);
                }
            });
        egui::Panel::right("sim")
            .resizable(true)
            .default_size(520.0)
            .size_range(320.0..=1100.0)
            .frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(8.0))
            .show(ui, |ui| self.sim_pane(ui));
        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(8, 6)))
            .show(ui, |ui| self.editor_pane(ui));

        self.unsaved_dialog(&ctx);
        self.capture_tick(&ctx);

        // Keep the transient status message fading out on time.
        if self.status.as_ref().is_some_and(|(_, t)| t.elapsed() < Duration::from_secs(4)) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
