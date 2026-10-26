//! Tape visualiser with three modes: numeric, heat map and usage bars.

use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, vec2};

use super::theme;
use crate::core::Machine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Numeric,
    Heat,
    Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumFmt {
    Dec,
    Hex,
    Char,
}

pub struct MemoryView {
    pub mode: Mode,
    pub num_fmt: NumFmt,
    pub follow_ptr: bool,
    last_ptr_row: Option<usize>,
}

impl Default for MemoryView {
    fn default() -> Self {
        Self { mode: Mode::Numeric, num_fmt: NumFmt::Dec, follow_ptr: true, last_ptr_row: None }
    }
}

const CELL: egui::Vec2 = egui::vec2(46.0, 40.0);
const GAP: f32 = 3.0;

impl MemoryView {
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (mode, label, tip) in [
                (Mode::Numeric, "0x Numeric", "cell values as numbers"),
                (Mode::Heat, "▦ Heat", "colour ramp from 0 to the tape's max value"),
                (Mode::Usage, "▮ Usage", "bars by how often each cell was touched"),
            ] {
                if tab(ui, self.mode == mode, label).on_hover_text(tip).clicked() {
                    self.mode = mode;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.follow_ptr, "follow ptr");
                if self.mode != Mode::Usage {
                    egui::ComboBox::from_id_salt("numfmt")
                        .width(56.0)
                        .selected_text(match self.num_fmt {
                            NumFmt::Dec => "dec",
                            NumFmt::Hex => "hex",
                            NumFmt::Char => "chr",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.num_fmt, NumFmt::Dec, "dec");
                            ui.selectable_value(&mut self.num_fmt, NumFmt::Hex, "hex");
                            ui.selectable_value(&mut self.num_fmt, NumFmt::Char, "chr");
                        });
                }
            });
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, vm: Option<&Machine>) {
        let Some(vm) = vm else {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("[ tape is empty ]").color(theme::TEXT_FAINT));
                ui.label(egui::RichText::new("press ▶ Run or ⏭ Step to start").color(theme::TEXT_FAINT).small());
            });
            return;
        };

        let max_val = vm.max_cell();
        let max_use = vm.usage.iter().copied().max().unwrap_or(0);
        if self.mode == Mode::Usage {
            self.usage_summary(ui, vm, max_use);
        }

        let cols = (((ui.available_width() + GAP) / (CELL.x + GAP)).floor() as usize).max(1);
        let shown = (vm.high_water.max(vm.ptr) + 1).max(cols * 4).min(vm.tape.len());
        let rows = shown.div_ceil(cols);
        let ptr_row = vm.ptr / cols;
        let row_h = CELL.y + GAP;

        let mut area = egui::ScrollArea::vertical().id_salt("tape-scroll").auto_shrink([false, false]);
        if self.follow_ptr && self.last_ptr_row != Some(ptr_row) {
            area = area.vertical_scroll_offset((ptr_row as f32 * row_h - row_h * 2.0).max(0.0));
        }
        self.last_ptr_row = Some(ptr_row);

        area.show_rows(ui, CELL.y, rows, |ui, range| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for row in range {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let i = row * cols + col;
                        if i >= shown {
                            break;
                        }
                        self.cell(ui, vm, i, max_val, max_use);
                    }
                });
            }
        });
    }

    fn usage_summary(&self, ui: &mut egui::Ui, vm: &Machine, max_use: u32) {
        let used = vm.usage.iter().filter(|u| **u > 0).count();
        let frac = used as f32 / vm.tape.len() as f32;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("cells used {used}/{}", vm.tape.len())).color(theme::TEXT_DIM).small());
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width() - 8.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, 2.0, theme::BG_CELL);
            // Visible minimum so a tiny fraction still shows up.
            let w = (r.width() * frac).max(if used > 0 { 3.0 } else { 0.0 });
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(w, r.height())), 2.0, theme::ORANGE);
        });
        if let Some((hot, n)) = vm.usage.iter().enumerate().max_by_key(|(_, n)| **n).filter(|(_, n)| **n > 0) {
            ui.label(
                egui::RichText::new(format!("hottest: cell #{hot} ({n} accesses)  ·  max {max_use}"))
                    .color(theme::TEXT_FAINT)
                    .small(),
            );
        }
        ui.add_space(4.0);
    }

    fn cell(&self, ui: &mut egui::Ui, vm: &Machine, i: usize, max_val: u32, max_use: u32) {
        let (rect, resp) = ui.allocate_exact_size(CELL, Sense::hover());
        let p = ui.painter();
        let v = vm.tape[i];
        let uses = vm.usage.get(i).copied().unwrap_or(0);
        let is_ptr = i == vm.ptr;
        let r = CornerRadius::same(3);
        let small = theme::mono(9.0);

        match self.mode {
            Mode::Numeric => {
                let bg = if is_ptr { Color32::from_rgb(0x4a, 0x2a, 0x1a) } else { theme::BG_CELL };
                p.rect_filled(rect, r, bg);
                let col = if v == 0 { theme::TEXT_FAINT } else { theme::TEXT };
                p.text(rect.center() - vec2(0.0, 4.0), Align2::CENTER_CENTER, self.fmt(v), theme::mono(13.0), col);
            }
            Mode::Heat => {
                let t = if max_val == 0 { 0.0 } else { v as f32 / max_val as f32 };
                let bg = theme::heat(t);
                p.rect_filled(rect, r, bg);
                p.text(rect.center() - vec2(0.0, 4.0), Align2::CENTER_CENTER, self.fmt(v), theme::mono(11.0), theme::text_on(bg));
            }
            Mode::Usage => {
                p.rect_filled(rect, r, theme::BG);
                let inner = rect.shrink2(vec2(6.0, 4.0));
                let bar_area = Rect::from_min_max(inner.min, egui::pos2(inner.max.x, inner.max.y - 10.0));
                p.rect_filled(bar_area, 1.0, theme::BG_CELL);
                if uses > 0 {
                    // Log scale so one hot loop doesn't flatten everything.
                    let t = (1.0 + uses as f32).ln() / (1.0 + max_use as f32).ln();
                    let h = (bar_area.height() * t).max(2.0);
                    let bar = Rect::from_min_max(egui::pos2(bar_area.min.x, bar_area.max.y - h), bar_area.max);
                    p.rect_filled(bar, 1.0, theme::lerp(theme::DEEP_RED, theme::ORANGE, t));
                }
            }
        }

        // Cell index along the bottom.
        let idx_col = if is_ptr { theme::ORANGE } else { theme::TEXT_FAINT };
        p.text(egui::pos2(rect.center().x, rect.max.y - 2.0), Align2::CENTER_BOTTOM, i.to_string(), small, idx_col);

        if is_ptr {
            p.rect_stroke(rect, r, Stroke::new(2.0, theme::ORANGE), StrokeKind::Inside);
        }

        resp.on_hover_ui(|ui| {
            let ch = char::from_u32(v).filter(|c| !c.is_control()).map_or("·".to_owned(), |c| format!("'{c}'"));
            ui.label(egui::RichText::new(format!("cell #{i}{}", if is_ptr { "  ◀ ptr" } else { "" })).color(theme::ORANGE));
            ui.label(format!("dec {v}   hex 0x{v:02X}   chr {ch}"));
            if !vm.usage.is_empty() {
                ui.label(format!("accesses {uses}"));
            }
        });
    }

    fn fmt(&self, v: u32) -> String {
        match self.num_fmt {
            NumFmt::Dec => v.to_string(),
            NumFmt::Hex => format!("{v:02X}"),
            NumFmt::Char => match char::from_u32(v) {
                Some(c) if c.is_ascii_graphic() => c.to_string(),
                Some(' ') => "␠".into(),
                Some('\n') => "↵".into(),
                _ if v == 0 => "·".into(),
                _ => format!("{v}"),
            },
        }
    }
}

/// A flat VS Code-style tab with an orange underline when selected.
pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let color = if selected { Color32::WHITE } else { theme::TEXT_DIM };
    let resp = ui.add(egui::Button::new(egui::RichText::new(label).color(color)).frame(false));
    if selected {
        let r = resp.rect;
        ui.painter().hline(r.x_range(), r.max.y + 1.0, Stroke::new(2.0, theme::ORANGE));
    }
    resp
}
