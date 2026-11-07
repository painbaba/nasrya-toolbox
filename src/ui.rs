//! Main dashboard rendering.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Mode, ToolStatus};
use crate::banner;

// Cohesive palette: cyan primary with semantic accents.
const CYAN: Color = Color::Rgb(80, 210, 230);
const CYAN_DIM: Color = Color::Rgb(45, 120, 135);
const GREEN: Color = Color::Rgb(120, 230, 150);
const RED: Color = Color::Rgb(255, 110, 120);
const YELLOW: Color = Color::Rgb(240, 215, 120);
const DIM: Color = Color::Rgb(95, 110, 130);
const FG: Color = Color::Rgb(215, 225, 235);
const INK: Color = Color::Rgb(10, 14, 20);

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),    // body
            Constraint::Length(1), // footer / input
        ])
        .split(area);

    render_header(f, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(chunks[1]);

    render_tool_list(f, app, body[0]);
    render_output(f, app, body[1]);
    render_footer(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(28)])
        .split(area);

    f.render_widget(Paragraph::new(banner::wordmark()), cols[0]);

    let installed = app
        .status
        .iter()
        .filter(|s| **s == ToolStatus::Installed)
        .count();
    let missing = app
        .status
        .iter()
        .filter(|s| **s == ToolStatus::Missing)
        .count();

    let stats = Line::from(vec![
        Span::styled("●", Style::default().fg(GREEN)),
        Span::styled(format!(" {installed} "), Style::default().fg(FG)),
        Span::styled("○", Style::default().fg(RED)),
        Span::styled(format!(" {missing} "), Style::default().fg(FG)),
        Span::styled(
            format!("· {} tools ", app.tools.len()),
            Style::default().fg(DIM),
        ),
    ])
    .alignment(Alignment::Right);
    f.render_widget(Paragraph::new(stats), cols[1]);
}

fn render_tool_list(f: &mut Frame, app: &App, area: Rect) {
    // Pad names to the longest one so the category column stays aligned.
    let name_w = app
        .tools
        .iter()
        .map(|t| t.name.chars().count())
        .max()
        .unwrap_or(0);
    let items: Vec<ListItem> = app
        .tools
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (icon, color) = match app.status[i] {
                ToolStatus::Installed => ("●", GREEN),
                ToolStatus::Missing => ("○", RED),
                ToolStatus::Unknown => ("·", DIM),
            };
            let name_style = Style::default().fg(FG).add_modifier(Modifier::BOLD);
            let line = Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(format!("{:<name_w$}", t.name), name_style),
                Span::styled(format!(" {}", t.category), Style::default().fg(DIM)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let pos = format!(" {}/{} ", app.selected + 1, app.tools.len().max(1));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(CYAN_DIM))
                .title(Span::styled(
                    " tools ",
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                ))
                .title_bottom(
                    Line::from(Span::styled(pos, Style::default().fg(DIM)))
                        .alignment(Alignment::Right),
                ),
        )
        .highlight_style(
            Style::default()
                .bg(CYAN)
                .fg(INK)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_output(f: &mut Frame, app: &App, area: Rect) {
    let tool = &app.tools[app.selected];

    let title = Line::from(vec![
        Span::styled(
            format!(" {} ", tool.name),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {} ", tool.description),
            Style::default().fg(DIM),
        ),
    ]);

    // Tail the scrollback to roughly fit the visible height.
    let inner_height = area.height.saturating_sub(2) as usize;
    let text: Vec<Line> = if app.output.is_empty() {
        vec![
            Line::from(Span::styled(
                "no output yet".to_string(),
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )),
            Line::from(Span::styled(
                "press Enter to run · i to install".to_string(),
                Style::default().fg(CYAN_DIM),
            )),
        ]
    } else {
        let start = app.output.len().saturating_sub(inner_height.max(1));
        app.output[start..]
            .iter()
            .map(|l| Line::from(style_output_line(l)))
            .collect()
    };

    let bottom = if app.running {
        let frame = SPINNER[(app.tick / 2 % SPINNER.len() as u64) as usize];
        let pid = app
            .running_pid
            .map(|p| format!("pid {p} "))
            .unwrap_or_default();
        Line::from(vec![
            Span::styled(
                format!(" {frame} running "),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled(pid, Style::default().fg(DIM)),
        ])
    } else {
        Line::from(Span::styled(" idle ", Style::default().fg(DIM)))
    };

    let border_color = if app.running { YELLOW } else { CYAN_DIM };

    let para = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(title)
                .title_bottom(bottom),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn style_output_line(l: &str) -> Span<'static> {
    if l.starts_with("[err]") {
        Span::styled(l.to_string(), Style::default().fg(RED))
    } else if l.starts_with("$ ") {
        Span::styled(
            l.to_string(),
            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        )
    } else if l.starts_with("──") || l.starts_with("::") {
        Span::styled(
            l.to_string(),
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(l.to_string(), Style::default().fg(FG))
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let content = match app.mode {
        Mode::Input => {
            let cursor = if (app.tick / 4) % 2 == 0 { "█" } else { " " };
            Line::from(vec![
                Span::styled(
                    " args ▸ ",
                    Style::default()
                        .fg(INK)
                        .bg(YELLOW)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", app.input), Style::default().fg(FG)),
                Span::styled(cursor.to_string(), Style::default().fg(YELLOW)),
                Span::styled(
                    "   (Enter to run · Esc to cancel)",
                    Style::default().fg(DIM),
                ),
            ])
        }
        Mode::Browsing => Line::from(vec![
            Span::styled(
                format!(" {} ", app.status_line),
                Style::default()
                    .fg(INK)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled("↑/↓", Style::default().fg(CYAN)),
            Span::styled(" move · ", Style::default().fg(DIM)),
            Span::styled("Enter", Style::default().fg(CYAN)),
            Span::styled(" run · ", Style::default().fg(DIM)),
            Span::styled("i", Style::default().fg(CYAN)),
            Span::styled(" install · ", Style::default().fg(DIM)),
            Span::styled("r", Style::default().fg(CYAN)),
            Span::styled(" re-detect · ", Style::default().fg(DIM)),
            Span::styled("c", Style::default().fg(CYAN)),
            Span::styled(" cancel · ", Style::default().fg(DIM)),
            Span::styled("q", Style::default().fg(CYAN)),
            Span::styled(" quit", Style::default().fg(DIM)),
        ]),
    };
    f.render_widget(Paragraph::new(content), area);
}
