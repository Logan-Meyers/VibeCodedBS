use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

use crate::db::{CachedEmail, Database};

/// Which panel currently has focus
#[derive(PartialEq)]
enum Focus {
    EmailList,
    EmailBody,
}

pub struct App {
    emails: Vec<CachedEmail>,
    list_state: ListState,
    focus: Focus,
    scroll_offset: u16,
    status_msg: Option<String>,
}

impl App {
    pub fn new(emails: Vec<CachedEmail>) -> Self {
        let mut list_state = ListState::default();
        if !emails.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            emails,
            list_state,
            focus: Focus::EmailList,
            scroll_offset: 0,
            status_msg: None,
        }
    }

    fn selected_email(&self) -> Option<&CachedEmail> {
        self.list_state.selected().and_then(|i| self.emails.get(i))
    }

    fn next_email(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(self.emails.len().saturating_sub(1)),
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    fn prev_email(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }
}

pub fn run(db: &Database) -> Result<()> {
    let emails = db.list_inbox(100)?;
    let mut app = App::new(emails);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| render(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match (&app.focus, key.code) {
                    // Quit
                    (_, KeyCode::Char('q')) => break,
                    (_, KeyCode::Char('c')) if key.modifiers.contains(KeyModifiers::CONTROL) => break,

                    // Navigation — email list
                    (Focus::EmailList, KeyCode::Char('j') | KeyCode::Down) => app.next_email(),
                    (Focus::EmailList, KeyCode::Char('k') | KeyCode::Up) => app.prev_email(),

                    // Switch focus to body with Enter or Tab
                    (Focus::EmailList, KeyCode::Enter | KeyCode::Tab) => {
                        app.focus = Focus::EmailBody;
                        app.scroll_offset = 0;
                    }

                    // Scroll body
                    (Focus::EmailBody, KeyCode::Char('j') | KeyCode::Down) => app.scroll_down(),
                    (Focus::EmailBody, KeyCode::Char('k') | KeyCode::Up) => app.scroll_up(),

                    // Back to list
                    (Focus::EmailBody, KeyCode::Esc | KeyCode::Tab) => {
                        app.focus = Focus::EmailList;
                    }

                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn render(f: &mut Frame, app: &mut App) {
    let area = f.size();

    // Split: left list | right body
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    render_email_list(f, app, chunks[0]);
    render_email_body(f, app, chunks[1]);
    render_help_bar(f, area);
}

fn render_email_list(f: &mut Frame, app: &mut App, area: Rect) {
    // Leave bottom row for help bar
    let area = Rect { height: area.height.saturating_sub(1), ..area };

    let focused = app.focus == Focus::EmailList;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .emails
        .iter()
        .map(|e| {
            let from = e
                .from_name
                .as_deref()
                .or(e.from_addr.as_deref())
                .unwrap_or("Unknown");

            let subject = e.subject.as_deref().unwrap_or("(no subject)");
            let date = e.received_at.as_deref().unwrap_or("").get(..10).unwrap_or("");

            let unread_marker = if !e.is_read { "● " } else { "  " };

            let line = Line::from(vec![
                Span::styled(unread_marker, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:<20}", truncate(from, 20)),
                    Style::default().fg(if e.is_read { Color::Gray } else { Color::White })
                        .add_modifier(if e.is_read { Modifier::empty() } else { Modifier::BOLD }),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:<40}", truncate(subject, 40)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("  {}", date),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" 📬 Inbox ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_email_body(f: &mut Frame, app: &mut App, area: Rect) {
    let area = Rect { height: area.height.saturating_sub(1), ..area };

    let focused = app.focus == Focus::EmailBody;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let content = match app.selected_email() {
        None => "No email selected.".to_string(),
        Some(email) => {
            let subject = email.subject.as_deref().unwrap_or("(no subject)");
            let from = email
                .from_name
                .as_deref()
                .unwrap_or(email.from_addr.as_deref().unwrap_or("Unknown"));
            let date = email.received_at.as_deref().unwrap_or("Unknown date");
            let body = email.body.as_deref().unwrap_or(
                email.preview.as_deref().unwrap_or("(no content)"),
            );

            // Strip basic HTML tags for plain text display
            let plain_body = strip_html(body);

            format!(
                "Subject: {}\nFrom:    {}\nDate:    {}\n{}\n\n{}",
                subject,
                from,
                date,
                "─".repeat(60),
                plain_body
            )
        }
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(" ✉  Email ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    f.render_widget(paragraph, area);
}

fn render_help_bar(f: &mut Frame, area: Rect) {
    let help = Paragraph::new(
        " j/k: navigate  Tab: switch panel  Enter: open  q: quit",
    )
    .style(Style::default().fg(Color::DarkGray).bg(Color::Black));

    let bar = Rect {
        y: area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    f.render_widget(help, bar);
}

/// Naive HTML tag stripper — good enough for PoC
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Collapse excessive blank lines
    let mut result = String::new();
    let mut blank_count = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
