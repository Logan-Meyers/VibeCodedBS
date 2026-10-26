//! Simulation controller: owns the VM, paces execution per frame, and draws
//! the run controls, stdin/stdout console and status.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use super::{editor::Document, memory_view::tab, theme};
use crate::core::{CellWidth, EofBehavior, Machine, ParseOptions, Status, VmConfig, parse_with, parser};

#[derive(Debug, Clone, PartialEq)]
pub enum RunState {
    Idle,
    Running,
    Paused(String),
    WaitingInput,
    Halted,
    Error(String),
}

impl RunState {
    pub fn label(&self) -> (&'static str, egui::Color32) {
        match self {
            RunState::Idle => ("○ idle", theme::TEXT_DIM),
            RunState::Running => ("● running", theme::OK),
            RunState::Paused(_) => ("⏸ paused", theme::AMBER),
            RunState::WaitingInput => ("⌨ waiting for input", theme::HINT),
            RunState::Halted => ("■ halted", theme::TEXT),
            RunState::Error(_) => ("✖ error", theme::RED),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleTab {
    Output,
    Input,
}

pub struct Sim {
    pub vm: Option<Machine>,
    pub state: RunState,
    pub config: VmConfig,
    pub hash_comments: bool,
    /// log10 of ops per second; the top of the range means turbo.
    pub speed_log: f32,
    pub console: String,
    pub stdin_line: String,
    /// Input typed before the program starts; fed on run.
    pub stdin_prefill: String,
    pub append_newline: bool,
    pub console_tab: ConsoleTab,
    /// Breakpoint flags straight from the parser (in-file markers).
    base_breaks: Vec<bool>,
    /// Document version the VM was built from.
    built_from: u64,
    budget_carry: f64,
    last_tick: Option<Instant>,
    pub ops_per_sec: f64,
    rate_window: (Instant, u64),
}

pub const TURBO: f32 = 9.0;

impl Default for Sim {
    fn default() -> Self {
        Self {
            vm: None,
            state: RunState::Idle,
            config: VmConfig { track_usage: true, ..Default::default() },
            hash_comments: true,
            speed_log: 3.0,
            console: String::new(),
            stdin_line: String::new(),
            stdin_prefill: String::new(),
            append_newline: true,
            console_tab: ConsoleTab::Output,
            base_breaks: Vec::new(),
            built_from: u64::MAX,
            budget_carry: 0.0,
            last_tick: None,
            ops_per_sec: 0.0,
            rate_window: (Instant::now(), 0),
        }
    }
}

impl Sim {
    pub fn parse_options(&self) -> ParseOptions {
        ParseOptions { hash_comments: self.hash_comments, optimize: true, split_lines: true }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, RunState::Running)
    }

    /// Throw away the VM (e.g. after an edit). Returns true if one existed.
    pub fn invalidate(&mut self) -> bool {
        self.state = RunState::Idle;
        self.vm.take().is_some()
    }

    /// Make sure a VM exists for the current document; returns false on a
    /// syntax error.
    fn ensure_vm(&mut self, doc: &Document) -> bool {
        if self.vm.is_some() && self.built_from == doc.version {
            return true;
        }
        match parse_with(&doc.text, self.parse_options(), &[]) {
            Ok(program) => {
                self.base_breaks = program.breaks.clone();
                let mut vm = Machine::new(program, self.config);
                vm.feed_input(self.stdin_prefill.as_bytes());
                self.vm = Some(vm);
                self.built_from = doc.version;
                self.console.clear();
                self.sync_breakpoints(doc);
                true
            }
            Err(e) => {
                self.state = RunState::Error(format!("syntax error {e}"));
                false
            }
        }
    }

    /// Re-apply gutter breakpoints to the live program.
    pub fn sync_breakpoints(&mut self, doc: &Document) {
        if let Some(vm) = &mut self.vm {
            let lines: Vec<usize> = doc.gutter_breaks.iter().copied().collect();
            vm.program.breaks = parser::breaks_for_lines(&vm.program.lines, &self.base_breaks, &lines);
        }
    }

    pub fn run(&mut self, doc: &Document) {
        if matches!(self.state, RunState::Halted | RunState::Error(_)) {
            self.vm = None;
        }
        if self.ensure_vm(doc) {
            self.state = RunState::Running;
            self.last_tick = None;
            self.budget_carry = 0.0;
        }
    }

    pub fn pause(&mut self) {
        if self.is_running() {
            self.state = RunState::Paused("paused".into());
        }
    }

    pub fn step(&mut self, doc: &Document) {
        if matches!(self.state, RunState::Halted | RunState::Error(_)) {
            self.vm = None;
        }
        if !self.ensure_vm(doc) {
            return;
        }
        let vm = self.vm.as_mut().unwrap();
        let status = vm.step();
        self.drain_output();
        self.apply_status(status, true);
    }

    pub fn reset(&mut self) {
        self.vm = None;
        self.state = RunState::Idle;
        self.console.clear();
        self.ops_per_sec = 0.0;
    }

    pub fn send_input(&mut self) {
        let mut bytes = self.stdin_line.clone().into_bytes();
        if self.append_newline {
            bytes.push(b'\n');
        }
        self.stdin_line.clear();
        match &mut self.vm {
            Some(vm) => {
                vm.feed_input(&bytes);
                // Echo, terminal style.
                self.console.push_str(&String::from_utf8_lossy(&bytes));
                if self.state == RunState::WaitingInput {
                    self.state = RunState::Running;
                }
            }
            None => self.stdin_prefill.push_str(&String::from_utf8_lossy(&bytes)),
        }
    }

    pub fn send_eof(&mut self) {
        if let Some(vm) = &mut self.vm {
            vm.close_input();
            if self.state == RunState::WaitingInput {
                self.state = RunState::Running;
            }
        }
    }

    fn drain_output(&mut self) {
        if let Some(vm) = &mut self.vm
            && !vm.output.is_empty()
        {
            let out = vm.take_output();
            self.console.push_str(&String::from_utf8_lossy(&out));
            // Keep the console bounded.
            if self.console.len() > 200_000 {
                let cut = self.console.len() - 150_000;
                let cut = (cut..self.console.len()).find(|&i| self.console.is_char_boundary(i)).unwrap_or(0);
                self.console.drain(..cut);
            }
        }
    }

    fn apply_status(&mut self, status: Status, stepping: bool) {
        let line = self.vm.as_ref().and_then(Machine::current_line).map_or(0, |l| l + 1);
        self.state = match status {
            Status::Running if stepping => RunState::Paused(format!("stepped → line {line}")),
            Status::Running => RunState::Running,
            Status::Breakpoint => RunState::Paused(format!("breakpoint hit at line {line}")),
            Status::NeedInput => {
                self.console_tab = ConsoleTab::Input;
                RunState::WaitingInput
            }
            Status::Halted => RunState::Halted,
            Status::Error(e) => RunState::Error(format!("{e} near line {line}")),
        };
    }

    /// Advance the simulation for this frame. Call once per frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.is_running() {
            self.last_tick = None;
            return;
        }
        let now = Instant::now();
        let dt = self.last_tick.map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f64()).min(0.1);
        self.last_tick = Some(now);
        let Some(vm) = &mut self.vm else { return };
        let status = if self.speed_log >= TURBO {
            // Turbo: burn ~12ms per frame in chunks.
            let deadline = now + Duration::from_millis(12);
            loop {
                let s = vm.run(200_000);
                if s != Status::Running || Instant::now() >= deadline {
                    break s;
                }
            }
        } else {
            let want = 10f64.powf(self.speed_log as f64) * dt + self.budget_carry;
            let budget = want.floor();
            self.budget_carry = want - budget;
            if budget >= 1.0 { vm.run(budget as u64) } else { Status::Running }
        };

        // Smoothed ops/sec readout.
        let (t0, n0) = self.rate_window;
        if now.duration_since(t0) > Duration::from_millis(400) {
            self.ops_per_sec = (vm.steps - n0) as f64 / now.duration_since(t0).as_secs_f64();
            self.rate_window = (now, vm.steps);
        }
        self.drain_output();
        self.apply_status(status, false);
        if self.is_running() {
            ctx.request_repaint();
        }
    }

    /// Run controls + speed slider.
    pub fn controls(&mut self, ui: &mut egui::Ui, doc: &Document) {
        ui.horizontal(|ui| {
            let running = self.is_running();
            let run_label = if matches!(self.state, RunState::Paused(_)) { "▶ Continue" } else { "▶ Run" };
            if ui
                .add_enabled(!running, accent_button(run_label))
                .on_hover_text("F5")
                .clicked()
            {
                self.run(doc);
            }
            if ui.add_enabled(running, egui::Button::new("⏸ Pause")).on_hover_text("F6").clicked() {
                self.pause();
            }
            if ui.add_enabled(!running, egui::Button::new("⏭ Step")).on_hover_text("F10").clicked() {
                self.step(doc);
            }
            if ui.button("⟲ Reset").on_hover_text("Shift+F5").clicked() {
                self.reset();
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("speed").color(theme::TEXT_DIM));
            ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(60.0);
            ui.add(egui::Slider::new(&mut self.speed_log, 0.0..=TURBO).show_value(false));
            let label = if self.speed_log >= TURBO {
                RichText::new("TURBO").color(theme::RED).strong()
            } else {
                RichText::new(format!("{} op/s", human(10f64.powf(self.speed_log as f64)))).color(theme::ORANGE)
            };
            ui.label(label);
        });
    }

    /// Settings that require a fresh VM.
    pub fn settings(&mut self, ui: &mut egui::Ui) {
        let before = (self.config, self.hash_comments);
        egui::Grid::new("vm-settings").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("cell size");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.config.cell, CellWidth::U8, "8-bit");
                ui.selectable_value(&mut self.config.cell, CellWidth::U16, "16-bit");
                ui.selectable_value(&mut self.config.cell, CellWidth::U32, "32-bit");
            });
            ui.end_row();
            ui.label("on EOF");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.config.eof, EofBehavior::Zero, "0");
                ui.selectable_value(&mut self.config.eof, EofBehavior::MinusOne, "-1");
                ui.selectable_value(&mut self.config.eof, EofBehavior::Unchanged, "keep");
            });
            ui.end_row();
            ui.label("tape");
            ui.add(egui::DragValue::new(&mut self.config.tape_len).range(1..=1_000_000).suffix(" cells"));
            ui.end_row();
            ui.label("pointer");
            ui.checkbox(&mut self.config.wrap_pointer, "wrap at ends");
            ui.end_row();
            ui.label("comments");
            ui.checkbox(&mut self.hash_comments, "`#` starts a comment");
            ui.end_row();
        });
        if before != (self.config, self.hash_comments) {
            self.reset();
        }
    }

    /// Output console and stdin box.
    pub fn console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if tab(ui, self.console_tab == ConsoleTab::Output, "stdout").clicked() {
                self.console_tab = ConsoleTab::Output;
            }
            let waiting = self.state == RunState::WaitingInput;
            let input_label = if waiting { "stdin ●" } else { "stdin" };
            if tab(ui, self.console_tab == ConsoleTab::Input, input_label).clicked() {
                self.console_tab = ConsoleTab::Input;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("clear").clicked() {
                    self.console.clear();
                }
            });
        });

        // Input row is always visible when the program is waiting.
        if self.console_tab == ConsoleTab::Input || self.state == RunState::WaitingInput {
            ui.horizontal(|ui| {
                ui.label(RichText::new(">").color(theme::ORANGE).strong());
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.stdin_line)
                        .hint_text(if self.vm.is_some() { "type input, Enter to send" } else { "pre-load input before running" })
                        .desired_width(ui.available_width() - 130.0),
                );
                if self.state == RunState::WaitingInput && !resp.has_focus() {
                    resp.request_focus();
                }
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("send").clicked() || enter {
                    self.send_input();
                    resp.request_focus();
                }
                if ui.button("EOF").on_hover_text("close stdin").clicked() {
                    self.send_eof();
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.append_newline, "append \\n");
                if !self.stdin_prefill.is_empty() {
                    ui.label(RichText::new(format!("queued: {:?}", self.stdin_prefill)).color(theme::TEXT_DIM).small());
                    if ui.small_button("×").clicked() {
                        self.stdin_prefill.clear();
                    }
                }
            });
        }

        egui::Frame::new().fill(theme::BG).inner_margin(6.0).corner_radius(2.0).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("console").stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                if self.console.is_empty() {
                    ui.label(RichText::new("// program output appears here").color(theme::TEXT_FAINT).italics());
                } else {
                    ui.add(egui::Label::new(RichText::new(&self.console).color(theme::TEXT)).wrap());
                }
                if let RunState::Error(e) = &self.state {
                    ui.label(RichText::new(format!("✖ {e}")).color(theme::RED));
                }
                if self.state == RunState::Halted {
                    ui.label(RichText::new("\n-- program halted --").color(theme::TEXT_FAINT));
                }
            });
        });
    }
}

pub fn accent_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(egui::Color32::WHITE).strong()).fill(theme::DEEP_RED)
}

/// 1234567 → "1.23M"
pub fn human(n: f64) -> String {
    match n {
        n if n >= 1e9 => format!("{:.2}G", n / 1e9),
        n if n >= 1e6 => format!("{:.2}M", n / 1e6),
        n if n >= 1e3 => format!("{:.1}k", n / 1e3),
        n => format!("{n:.0}"),
    }
}
