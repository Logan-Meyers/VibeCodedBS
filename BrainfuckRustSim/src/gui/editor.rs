//! Code editor: syntax highlighting, line-number/breakpoint gutter,
//! diagnostic underlines, current-op highlight and auto-indent.

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, Id, Key, Rect, Sense, Shape, Stroke, TextFormat,
    text::{CCursor, CCursorRange, LayoutJob},
};

use super::theme;
use crate::core::{Diagnostic, FormatOptions, Indent, Severity, parser};

pub struct Document {
    pub path: Option<PathBuf>,
    pub text: String,
    saved_text: String,
    /// Bumped on every edit so other panes can notice changes.
    pub version: u64,
    /// Breakpoints toggled in the gutter (0-based lines).
    pub gutter_breaks: BTreeSet<usize>,
}

impl Document {
    pub fn new(path: Option<PathBuf>, text: String) -> Self {
        Self { path, saved_text: text.clone(), text, version: 0, gutter_breaks: BTreeSet::new() }
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn mark_saved(&mut self) {
        self.saved_text = self.text.clone();
    }

    pub fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "untitled.bf".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Lines carrying an in-file `# BREAKPOINT` marker.
    pub fn marker_lines(&self) -> BTreeSet<usize> {
        self.text
            .lines()
            .enumerate()
            .filter(|(_, l)| parser::hash_comment_start(l).is_some_and(|i| parser::is_breakpoint_comment(&l[i..])))
            .map(|(i, _)| i)
            .collect()
    }
}

/// What the editor needs to know from the rest of the app this frame.
pub struct EditorContext<'a> {
    pub diagnostics: &'a [Diagnostic],
    /// Source span of the op about to execute.
    pub current_span: Option<Range<usize>>,
    pub current_line: Option<usize>,
    pub hash_comments: bool,
    pub font_size: f32,
    pub indent: Indent,
    /// Request to move the cursor to a byte offset (e.g. clicking a problem).
    pub jump_to: Option<usize>,
}

#[derive(Default)]
pub struct EditorOutput {
    pub changed: bool,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

const GUTTER_W: f32 = 58.0;

pub fn show(ui: &mut egui::Ui, doc: &mut Document, cx: EditorContext<'_>) -> EditorOutput {
    let mut out = EditorOutput::default();
    let font = theme::mono(cx.font_size);
    let edit_id = Id::new("bf-editor");
    let markers = doc.marker_lines();

    let enter = ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.any());
    let typed_close = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "]")));
    let tab = ui.input(|i| i.key_pressed(Key::Tab) && !i.modifiers.shift);

    if let Some(offset) = cx.jump_to {
        set_cursor(ui.ctx(), edit_id, char_index(&doc.text, offset));
        ui.memory_mut(|m| m.request_focus(edit_id));
    }

    egui::ScrollArea::both().id_salt("editor-scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let line_count = doc.text.split('\n').count().max(1);
            let row_h = ui.fonts_mut(|f| f.row_height(&font));
            let (gutter_rect, gutter_resp) =
                ui.allocate_exact_size(egui::vec2(GUTTER_W, row_h * line_count as f32 + 8.0), Sense::click());
            // Reserve a slot behind the text for line highlights.
            let bg_slot = ui.painter().add(Shape::Noop);

            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| -> Arc<egui::Galley> {
                let job = highlight(buf.as_str(), &cx, &font);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let output = egui::TextEdit::multiline(&mut doc.text)
                .id(edit_id)
                .code_editor()
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY)
                .desired_rows(40)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);

            let galley_pos = output.galley_pos;
            // One galley row per source line (wrapping is off).
            let row_h = (output.galley.size().y / line_count as f32).max(1.0);
            let line_y = |line: usize| galley_pos.y + line as f32 * row_h;
            let full_w = ui.clip_rect().max.x;

            // Current execution line + breakpoint line backgrounds.
            let mut bg = Vec::new();
            for &l in markers.iter().chain(doc.gutter_breaks.iter()) {
                let r = Rect::from_min_max(egui::pos2(gutter_rect.max.x, line_y(l)), egui::pos2(full_w, line_y(l) + row_h));
                bg.push(Shape::rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0xe8, 0x45, 0x3c, 22)));
            }
            if let Some(l) = cx.current_line {
                let r = Rect::from_min_max(egui::pos2(gutter_rect.max.x, line_y(l)), egui::pos2(full_w, line_y(l) + row_h));
                bg.push(Shape::rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0xff, 0x8c, 0x42, 40)));
                bg.push(Shape::line_segment(
                    [r.left_top(), r.left_bottom()],
                    Stroke::new(2.0, theme::ORANGE),
                ));
            }
            ui.painter().set(bg_slot, Shape::Vec(bg));

            // Gutter.
            let p = ui.painter_at(gutter_rect.expand2(egui::vec2(0.0, row_h)));
            p.rect_filled(gutter_rect.expand2(egui::vec2(0.0, 2000.0)), 0.0, theme::BG);
            let diag_lines: Vec<(usize, Severity)> = cx.diagnostics.iter().map(|d| (d.line, d.severity)).collect();
            for line in 0..line_count {
                let y = line_y(line);
                let is_cur = cx.current_line == Some(line);
                let color = if is_cur { theme::ORANGE } else { theme::TEXT_FAINT };
                p.text(
                    egui::pos2(gutter_rect.max.x - 12.0, y),
                    egui::Align2::RIGHT_TOP,
                    (line + 1).to_string(),
                    theme::mono(cx.font_size - 1.0),
                    color,
                );
                let dot = egui::pos2(gutter_rect.min.x + 9.0, y + row_h / 2.0);
                if markers.contains(&line) {
                    p.circle_filled(dot, 4.5, theme::RED);
                    p.circle_stroke(dot, 4.5, Stroke::new(1.0, theme::AMBER));
                } else if doc.gutter_breaks.contains(&line) {
                    p.circle_filled(dot, 4.5, theme::RED);
                }
                if is_cur {
                    p.text(egui::pos2(gutter_rect.max.x - 10.0, y), egui::Align2::LEFT_TOP, "▶", theme::mono(cx.font_size - 3.0), theme::ORANGE);
                }
                if let Some((_, sev)) = diag_lines.iter().filter(|(l, _)| *l == line).max_by_key(|(_, s)| *s) {
                    let c = severity_color(*sev);
                    p.rect_filled(
                        Rect::from_min_size(egui::pos2(gutter_rect.min.x + 17.0, y + 2.0), egui::vec2(2.0, row_h - 4.0)),
                        0.0,
                        c,
                    );
                }
            }
            // Click in the gutter toggles a breakpoint on that line.
            if gutter_resp.clicked()
                && let Some(pos) = gutter_resp.interact_pointer_pos()
            {
                let line = ((pos.y - galley_pos.y) / row_h).floor().max(0.0) as usize;
                if line < line_count && !doc.gutter_breaks.remove(&line) {
                    doc.gutter_breaks.insert(line);
                }
            }
            gutter_resp.on_hover_text("click to toggle a breakpoint");

            // Cursor position + auto-indent tweaks.
            if let Some(range) = output.cursor_range {
                let ci = range.primary.index.0;
                let changed = output.response.response.changed();
                let mut new_cursor = None;
                if changed && enter {
                    new_cursor = auto_indent(&mut doc.text, ci, cx.indent);
                } else if changed && typed_close {
                    new_cursor = auto_dedent(&mut doc.text, ci, cx.indent);
                } else if changed && tab && matches!(cx.indent, Indent::Spaces(_)) {
                    new_cursor = tab_to_spaces(&mut doc.text, ci, cx.indent);
                }
                let ci = new_cursor.unwrap_or(ci);
                if let Some(c) = new_cursor {
                    set_cursor(ui.ctx(), edit_id, c);
                }
                let byte = byte_index(&doc.text, ci);
                let (l, c) = parser::line_col(&doc.text, byte);
                out.cursor_line = l;
                out.cursor_col = c;
            }
            out.changed = output.response.response.changed();
        });
    });
    if out.changed {
        doc.version += 1;
    }
    out
}

pub fn severity_color(s: Severity) -> Color32 {
    match s {
        Severity::Error => theme::RED,
        Severity::Warning => theme::WARN,
        Severity::Hint => theme::HINT,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Arith,
    Move,
    Io,
    Bracket(usize),
    HashComment,
    Breakpoint,
}

/// Build the coloured layout for the whole buffer.
fn highlight(text: &str, cx: &EditorContext<'_>, font: &egui::FontId) -> LayoutJob {
    // Start empty: `append` below adds each section's text, so the job's text
    // ends up byte-for-byte equal to the buffer (TextEdit relies on that).
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;

    let mut depth: usize = 0;
    let mut kinds: Vec<(usize, Kind)> = Vec::with_capacity(text.len());
    let mut line_start = 0;
    for line in text.split('\n') {
        let hash = if cx.hash_comments { parser::hash_comment_start(line) } else { None };
        let is_bp = parser::hash_comment_start(line).is_some_and(|i| parser::is_breakpoint_comment(&line[i..]));
        for (i, ch) in line.char_indices() {
            let kind = if hash.is_some_and(|h| i >= h) || (is_bp && line[i..].starts_with('#')) {
                if is_bp { Kind::Breakpoint } else { Kind::HashComment }
            } else {
                match ch {
                    '+' | '-' => Kind::Arith,
                    '<' | '>' => Kind::Move,
                    '.' | ',' => Kind::Io,
                    '[' => {
                        depth += 1;
                        Kind::Bracket(depth - 1)
                    }
                    ']' => {
                        depth = depth.saturating_sub(1);
                        Kind::Bracket(depth)
                    }
                    _ => Kind::Text,
                }
            };
            kinds.push((line_start + i, kind));
        }
        line_start += line.len() + 1;
    }

    // Split points: kind changes, plus diagnostic and current-op boundaries.
    let mut cuts: Vec<usize> = vec![0, text.len()];
    let mut prev = None;
    for &(i, k) in &kinds {
        if prev != Some(k) {
            cuts.push(i);
            prev = Some(k);
        }
        // Brackets get their own section so pairs can differ.
        if matches!(k, Kind::Bracket(_)) {
            cuts.push(i);
            cuts.push(i + 1);
        }
    }
    let spans: Vec<(Range<usize>, Color32)> = cx
        .diagnostics
        .iter()
        .filter(|d| d.severity != Severity::Hint)
        .map(|d| (d.span.clone(), severity_color(d.severity)))
        .collect();
    for (r, _) in &spans {
        cuts.extend([r.start, r.end]);
    }
    if let Some(r) = &cx.current_span {
        cuts.extend([r.start, r.end]);
    }
    for (i, _) in text.char_indices() {
        if text.as_bytes()[i] == b'\n' {
            cuts.push(i);
        }
    }
    cuts.retain(|&c| c <= text.len() && text.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();

    let kind_at = |pos: usize| match kinds.binary_search_by_key(&pos, |(i, _)| *i) {
        Ok(i) => kinds[i].1,
        Err(_) => Kind::Text,
    };

    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        let kind = kind_at(a);
        let mut fmt = TextFormat { font_id: font.clone(), color: theme::TEXT, ..Default::default() };
        match kind {
            Kind::Text => fmt.color = theme::SYN_COMMENT,
            Kind::Arith => fmt.color = theme::SYN_ARITH,
            Kind::Move => fmt.color = theme::SYN_MOVE,
            Kind::Io => fmt.color = theme::SYN_IO,
            Kind::Bracket(d) => fmt.color = theme::BRACKETS[d % theme::BRACKETS.len()],
            Kind::HashComment => {
                fmt.color = theme::SYN_HASH_COMMENT;
                fmt.italics = true;
            }
            Kind::Breakpoint => {
                fmt.color = theme::RED;
                fmt.background = Color32::from_rgba_unmultiplied(0xe8, 0x45, 0x3c, 30);
            }
        }
        if let Some((_, c)) = spans.iter().find(|(r, _)| r.start <= a && a < r.end) {
            fmt.underline = Stroke::new(2.0, *c);
        }
        if cx.current_span.as_ref().is_some_and(|r| r.start <= a && a < r.end) {
            fmt.background = Color32::from_rgb(0x8a, 0x3d, 0x12);
            fmt.color = Color32::WHITE;
        }
        job.append(&text[a..b], 0.0, fmt);
    }
    if text.is_empty() {
        job.append("", 0.0, TextFormat { font_id: font.clone(), ..Default::default() });
    }
    job
}

fn set_cursor(ctx: &egui::Context, id: Id, char_idx: usize) {
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(char_idx))));
        state.store(ctx, id);
    }
}

fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

fn char_index(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx.min(s.len())].chars().count()
}

fn indent_unit(indent: Indent) -> String {
    match indent {
        Indent::Tab => "\t".into(),
        Indent::Spaces(n) => " ".repeat(n),
    }
}

/// After Enter: copy the previous line's indentation (+1 level after `[`).
fn auto_indent(text: &mut String, cursor: usize, indent: Indent) -> Option<usize> {
    let at = byte_index(text, cursor);
    if at == 0 || text.as_bytes()[at - 1] != b'\n' {
        return None;
    }
    let prev_start = text[..at - 1].rfind('\n').map_or(0, |i| i + 1);
    let prev = &text[prev_start..at - 1];
    let mut ws: String = prev.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let code = prev.split('#').next().unwrap_or("").trim_end();
    if code.ends_with('[') {
        ws.push_str(&indent_unit(indent));
    }
    if ws.is_empty() {
        return None;
    }
    text.insert_str(at, &ws);
    Some(cursor + ws.chars().count())
}

/// After typing `]` on an otherwise blank line: drop one indent level.
fn auto_dedent(text: &mut String, cursor: usize, indent: Indent) -> Option<usize> {
    let at = byte_index(text, cursor);
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..at];
    if !before.ends_with(']') || !before[..before.len() - 1].chars().all(|c| c == ' ' || c == '\t') {
        return None;
    }
    let ws = &before[..before.len() - 1];
    let unit = indent_unit(indent);
    let remove = if ws.ends_with(&unit) {
        unit.len()
    } else if ws.ends_with('\t') {
        1
    } else {
        ws.len().min(unit.len())
    };
    if remove == 0 {
        return None;
    }
    let cut = line_start + ws.len() - remove;
    text.replace_range(cut..cut + remove, "");
    Some(cursor - remove)
}

/// Replace a just-typed tab with spaces.
fn tab_to_spaces(text: &mut String, cursor: usize, indent: Indent) -> Option<usize> {
    let at = byte_index(text, cursor);
    if at == 0 || text.as_bytes()[at - 1] != b'\t' {
        return None;
    }
    let unit = indent_unit(indent);
    text.replace_range(at - 1..at, &unit);
    Some(cursor - 1 + unit.chars().count())
}

/// Format the document in place.
pub fn format_document(doc: &mut Document, opts: FormatOptions) -> bool {
    let formatted = crate::core::format(&doc.text, opts);
    if formatted != doc.text {
        doc.text = formatted;
        doc.version += 1;
        doc.gutter_breaks.clear();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(diags: &'a [Diagnostic], span: Option<Range<usize>>) -> EditorContext<'a> {
        EditorContext {
            diagnostics: diags,
            current_span: span,
            current_line: None,
            hash_comments: true,
            font_size: 14.0,
            indent: Indent::Spaces(4),
            jump_to: None,
        }
    }

    /// The galley must contain exactly the buffer's text, or TextEdit's
    /// cursor/char indices drift away from the real string.
    #[test]
    fn highlight_text_matches_buffer() {
        let font = theme::mono(14.0);
        let src = "# héllo ✨\n+++[>+<-]\n# BREAKPOINT\n.,\n\n]";
        let diags = crate::core::check(src, crate::core::ParseOptions::default());
        for text in ["", "+", "\n", src] {
            let job = highlight(text, &cx(&diags, Some(0..1)), &font);
            assert_eq!(job.text, text);
            // Sections tile the text with no gaps or overlaps.
            let mut end = 0;
            for s in &job.sections {
                assert_eq!(s.byte_range.start.0, end);
                end = s.byte_range.end.0;
            }
            assert_eq!(end, text.len());
        }
    }

    /// Drive the real TextEdit (with our layouter) through one frame.
    fn frame(ctx: &egui::Context, doc: &mut Document, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            show(ui, doc, cx(&[], None));
        });
        // No renderer here: discard font-atlas uploads.
        out.textures_delta.clear();
    }

    fn key(key: Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
    }

    #[test]
    fn backspace_and_paste_edit_at_the_cursor() {
        let ctx = egui::Context::default();
        let mut doc = Document::new(None, "+++\n>>>\n...".to_owned());
        frame(&ctx, &mut doc, vec![]);
        ctx.memory_mut(|m| m.request_focus(Id::new("bf-editor")));
        frame(&ctx, &mut doc, vec![]);

        // Cursor at the end of line 2 (char 7), then backspace.
        set_cursor(&ctx, Id::new("bf-editor"), 7);
        frame(&ctx, &mut doc, vec![key(Key::Backspace)]);
        assert_eq!(doc.text, "+++\n>>\n...");

        // Paste lands exactly at the cursor, not somewhere else.
        frame(&ctx, &mut doc, vec![egui::Event::Paste("[-]".into())]);
        assert_eq!(doc.text, "+++\n>>[-]\n...");

        // Typing too.
        frame(&ctx, &mut doc, vec![egui::Event::Text(",".into())]);
        assert_eq!(doc.text, "+++\n>>[-],\n...");
    }

    #[test]
    fn indent_after_open_bracket() {
        let mut s = String::from("  +[\n");
        let c = auto_indent(&mut s, 5, Indent::Spaces(4)).unwrap();
        assert_eq!(s, "  +[\n      ");
        assert_eq!(c, 11);
    }

    #[test]
    fn dedent_on_close() {
        let mut s = String::from("[\n    ]");
        let c = auto_dedent(&mut s, 7, Indent::Spaces(4)).unwrap();
        assert_eq!(s, "[\n]");
        assert_eq!(c, 3);
    }

    #[test]
    fn tabs_become_spaces() {
        let mut s = String::from("+\t");
        assert_eq!(tab_to_spaces(&mut s, 2, Indent::Spaces(2)), Some(3));
        assert_eq!(s, "+  ");
    }
}
