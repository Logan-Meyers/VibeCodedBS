pub mod file_browser;
pub mod status_panel;
pub mod export_popup;

use crate::app::{ActivePanel, App, AppMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.size();

    // Main horizontal split: left browser | right status+actions
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(size);

    // Right side: top status | bottom actions
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(12)])
        .split(main_chunks[1]);

    draw_browser(frame, app, main_chunks[0]);
    status_panel::draw_status(frame, app, right_chunks[0]);
    status_panel::draw_actions(frame, app, right_chunks[1]);
    draw_keybind_bar(frame, app, size);

    // Overlays
    if let AppMode::ExportPopup = &app.mode {
        export_popup::draw_export_popup(frame, app, size);
    }
    if let AppMode::ConfirmDialog { message, .. } = &app.mode {
        draw_confirm_dialog(frame, message, size);
    }
}

fn draw_browser(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Browser;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" 📁 Library ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = app
        .browser
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let indent = "  ".repeat(node.depth);
            let icon = if node.is_dir {
                if node.expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            let date_tag = if node.date_order_enabled { " [D]" } else { "" };
            let label = format!("{}{}{}{}", indent, icon, node.name, date_tag);

            let style = if i == app.browser.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if node.is_dir {
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.browser.selected));

    let list = List::new(items);
    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn draw_keybind_bar(frame: &mut Frame, app: &App, area: Rect) {
    let quit_str = app.config.keybinds.quit.to_string();
    let hints: &[(&str, &str)] = match app.active_panel {
        ActivePanel::Browser => &[
            ("hjkl", "navigate/expand"),
            ("Tab", "→ Actions"),
        ],
        ActivePanel::Actions => &[
            ("jk", "select action"),
            ("Enter", "run"),
            ("f/c/v/d/e/w", "shortcut"),
            ("Tab", "→ Browser"),
        ],
    };

    let mut spans = vec![Span::raw(" ")];
    for (key, desc) in hints {
        spans.push(Span::styled(key.to_string(), Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(format!(": {}  ", desc)));
    }
    spans.push(Span::styled(quit_str, Style::default().fg(Color::Yellow)));
    spans.push(Span::raw(": quit"));

    let bar = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray));

    let bar_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    frame.render_widget(bar, bar_area);
}

fn draw_confirm_dialog(frame: &mut Frame, message: &str, area: Rect) {
    use ratatui::widgets::Clear;
    let dialog_width = 50u16;
    let dialog_height = 5u16;
    let x = area.width.saturating_sub(dialog_width) / 2;
    let y = area.height.saturating_sub(dialog_height) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);
    let block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let text = Paragraph::new(format!("{}\n\n[y] Yes   [n] No", message))
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(text, dialog_area);
}
