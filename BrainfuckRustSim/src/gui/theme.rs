//! VS Code-ish dark palette with red/orange accents.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals, style::Selection,
};

pub const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x1e);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x25, 0x25, 0x26);
pub const BG_BAR: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2d, 0x2e);
pub const BG_CELL: Color32 = Color32::from_rgb(0x2d, 0x2d, 0x30);
pub const BORDER: Color32 = Color32::from_rgb(0x3c, 0x3c, 0x3c);
pub const TEXT: Color32 = Color32::from_rgb(0xd4, 0xd4, 0xd4);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x85, 0x85, 0x85);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);

pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x8c, 0x42);
pub const RED: Color32 = Color32::from_rgb(0xe8, 0x45, 0x3c);
pub const DEEP_RED: Color32 = Color32::from_rgb(0x9e, 0x2a, 0x2b);
pub const AMBER: Color32 = Color32::from_rgb(0xff, 0xc1, 0x4d);

// Syntax colours.
pub const SYN_ARITH: Color32 = Color32::from_rgb(0xff, 0x9d, 0x5c);
pub const SYN_MOVE: Color32 = Color32::from_rgb(0x56, 0x9c, 0xd6);
pub const SYN_IO: Color32 = Color32::from_rgb(0x4e, 0xc9, 0xb0);
pub const SYN_COMMENT: Color32 = Color32::from_rgb(0x6a, 0x99, 0x55);
pub const SYN_HASH_COMMENT: Color32 = Color32::from_rgb(0x60, 0x8b, 0x4e);
/// Bracket pair colours by nesting depth.
pub const BRACKETS: [Color32; 4] = [
    Color32::from_rgb(0xff, 0xd7, 0x00),
    Color32::from_rgb(0xda, 0x70, 0xd6),
    Color32::from_rgb(0x17, 0x9f, 0xff),
    Color32::from_rgb(0xff, 0x8c, 0x42),
];

pub const WARN: Color32 = Color32::from_rgb(0xcc, 0xa7, 0x00);
pub const HINT: Color32 = Color32::from_rgb(0x4f, 0xc1, 0xff);
pub const OK: Color32 = Color32::from_rgb(0x89, 0xd1, 0x85);

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        // Everything monospace: it's a code tool after all.
        style.text_styles = [
            (TextStyle::Heading, mono(15.0)),
            (TextStyle::Body, mono(13.0)),
            (TextStyle::Monospace, mono(13.0)),
            (TextStyle::Button, mono(13.0)),
            (TextStyle::Small, mono(10.5)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);

        let mut v = Visuals::dark();
        v.panel_fill = BG_PANEL;
        v.window_fill = BG_PANEL;
        v.extreme_bg_color = BG;
        v.faint_bg_color = BG_HOVER;
        v.code_bg_color = BG;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.hyperlink_color = ORANGE;
        v.warn_fg_color = WARN;
        v.error_fg_color = RED;
        v.selection = Selection { bg_fill: Color32::from_rgb(0x5a, 0x2e, 0x1c), stroke: Stroke::new(1.0, ORANGE) };
        v.window_corner_radius = CornerRadius::same(3);
        v.menu_corner_radius = CornerRadius::same(3);

        let r = CornerRadius::same(2);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = BG_PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.bg_fill = BG_BAR;
        w.inactive.weak_bg_fill = BG_BAR;
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.corner_radius = r;
        w.hovered.bg_fill = Color32::from_rgb(0x45, 0x3a, 0x33);
        w.hovered.weak_bg_fill = Color32::from_rgb(0x45, 0x3a, 0x33);
        w.hovered.bg_stroke = Stroke::new(1.0, ORANGE);
        w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        w.hovered.corner_radius = r;
        w.active.bg_fill = DEEP_RED;
        w.active.weak_bg_fill = DEEP_RED;
        w.active.bg_stroke = Stroke::new(1.0, RED);
        w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        w.active.corner_radius = r;
        w.open.bg_fill = BG_BAR;
        w.open.weak_bg_fill = BG_BAR;
        w.open.corner_radius = r;
        style.visuals = v;
    });
}

/// Heat-map ramp: 0 → dark cell, then deep red → orange → amber.
pub fn heat(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return BG_CELL;
    }
    let stops = [BG_CELL, DEEP_RED, RED, ORANGE, AMBER];
    let x = t * (stops.len() - 1) as f32;
    let i = (x.floor() as usize).min(stops.len() - 2);
    lerp(stops[i], stops[i + 1], x - i as f32)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Readable text colour on top of `bg`.
pub fn text_on(bg: Color32) -> Color32 {
    let lum = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if lum > 140.0 { Color32::from_rgb(0x1e, 0x1e, 0x1e) } else { TEXT }
}

/// A VS Code-style panel title: `// EXPLORER`.
pub fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("//").color(ORANGE).font(mono(11.0)));
        ui.label(egui::RichText::new(title).color(TEXT_DIM).font(mono(11.0)).strong());
    });
}
