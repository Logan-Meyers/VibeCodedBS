use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn draw_export_popup(frame: &mut Frame, app: &App, area: Rect) {
    let popup_width = (area.width as f32 * 0.6) as u16;
    let popup_height = 10u16;
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" 🎵 Export to iPod (Rockbox) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    // Placeholder — drive selection will be interactive in next iteration
    let lines = vec![
        Line::from(Span::styled(
            "Select iPod root drive:",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::raw("  (Drive selection coming soon)")),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "Layout: {}",
                app.config.export.rockbox_layout
            ),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            format!("Art format: {}", app.config.export.art_format.to_uppercase()),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[Esc] Close",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(para, popup_area);
}
