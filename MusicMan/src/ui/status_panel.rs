use crate::app::{ActivePanel, App};
use crate::ui::file_browser::FolderAction;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" ℹ Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let selected_info = app
        .browser
        .selected_path()
        .map(|p| {
            let node = &app.browser.nodes[app.browser.selected];
            let kind = if node.is_dir { "Directory" } else { "File" };
            let date_order = if node.date_order_enabled {
                " | Date-order: ON"
            } else {
                ""
            };
            format!("{}: {}{}", kind, p.display(), date_order)
        })
        .unwrap_or_else(|| "No selection".into());

    // Show last few status messages + selected path info
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            selected_info,
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
    ];

    for msg in app.status_messages.iter().rev().take(20).rev() {
        lines.push(Line::from(Span::raw(msg.clone())));
    }

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true });

    frame.render_widget(para, area);
}

pub fn draw_actions(frame: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Actions;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" ⚡ Actions (Tab to focus, Enter to run) ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let actions = app.selected_actions();

    if actions.is_empty() {
        let para = Paragraph::new("Select a file or folder to see actions.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(para, area);
        return;
    }

    let node_date_order = app
        .browser
        .nodes
        .get(app.browser.selected)
        .map(|n| n.date_order_enabled)
        .unwrap_or(false);

    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let label = match action {
                FolderAction::DateOrderTracknumbers => {
                    if node_date_order {
                        "[d] Date-order track numbers  ✓ ON".to_string()
                    } else {
                        "[d] Date-order track numbers".to_string()
                    }
                }
                _ => action.label().to_string(),
            };

            let style = if is_active && i == app.action_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let mut list_state = ListState::default();
    if is_active {
        list_state.select(Some(app.action_cursor));
    }

    let list = List::new(items).block(block);
    frame.render_stateful_widget(list, area, &mut list_state);
}
