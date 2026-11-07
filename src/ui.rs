//! Main dashboard rendering.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
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
    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let t = &app.tools[i];
            let (icon, color) = match app.status[i] {
                ToolStatus::Installed => ("●", GREEN),
                ToolStatus::Missing => ("○", RED),
                ToolStatus::Unknown => ("·", DIM),
                ToolStatus::Conflict => ("◐", YELLOW),
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

    let sel_pos = visible.iter().position(|&i| i == app.selected);
    let pos = match sel_pos {
        Some(p) => format!(" {}/{} ", p + 1, visible.len()),
        None => format!(" 0/{} ", visible.len()),
    };
    let mut title = vec![Span::styled(
        " tools ",
        Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
    )];
    if !app.filter.trim().is_empty() {
        title.push(Span::styled(
            format!(
                "/ {} ({} of {}) ",
                app.filter.trim(),
                visible.len(),
                app.tools.len()
            ),
            Style::default().fg(YELLOW),
        ));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(CYAN_DIM))
                .title(Line::from(title))
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
    state.select(sel_pos);
    f.render_stateful_widget(list, area, &mut state);
}

fn render_output(f: &mut Frame, app: &App, area: Rect) {
    let tool = &app.tools[app.selected];

    let title = Line::from(vec![
        Span::styled(
            format!(" {} ", tool.name),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("· {} ", tool.description), Style::default().fg(DIM)),
    ]);

    // Lines are hard-wrapped here (not by Paragraph) so we know exactly how
    // many rows the tail takes: with word-wrap, long lines used to push the
    // newest output below the pane.
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let (rows, max_scroll) = visible_rows(&app.output, inner_width, inner_height, app.scroll);
    app.view.set((inner_height, max_scroll, inner_width));
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
        rows.into_iter()
            .map(|(i, chunk)| {
                let mut style = line_style(&app.output[i]);
                if app.search_hit == Some(i) {
                    style = style
                        .bg(Color::Rgb(70, 60, 10))
                        .add_modifier(Modifier::BOLD);
                }
                Line::from(Span::styled(chunk, style))
            })
            .collect()
    };

    let mut bottom = if app.running {
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
    if app.scroll > 0 {
        bottom.push_span(Span::styled(
            format!(" ↑ {} rows up · End to follow ", app.scroll.min(max_scroll)),
            Style::default().fg(CYAN),
        ));
    }

    let border_color = if app.running { YELLOW } else { CYAN_DIM };

    let para = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_bottom(bottom),
    );
    f.render_widget(para, area);
}

/// Hard-wrap `output` to `width` columns and return the `height` rows that
/// are visible `scroll` rows above the bottom, as (source line index, text),
/// plus the largest useful scroll value. Only walks as far back as needed.
pub fn visible_rows(
    output: &[String],
    width: usize,
    height: usize,
    scroll: usize,
) -> (Vec<(usize, String)>, usize) {
    let width = width.max(1);
    let need = scroll + height;
    // rows collected bottom-up; past `need` rows we only count (for max_scroll)
    let mut rows: Vec<(usize, String)> = Vec::new();
    let mut total_rows = 0usize;
    for (i, line) in output.iter().enumerate().rev() {
        let chars: Vec<char> = line.chars().collect();
        let n = chars.len().div_ceil(width).max(1);
        total_rows += n;
        if rows.len() < need {
            let chunks: Vec<String> = if chars.is_empty() {
                vec![String::new()]
            } else {
                chars.chunks(width).map(|c| c.iter().collect()).collect()
            };
            for chunk in chunks.into_iter().rev() {
                rows.push((i, chunk));
            }
        }
    }
    let max_scroll = total_rows.saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let visible: Vec<(usize, String)> = rows
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (visible, max_scroll)
}

/// Colour for an output line, by its prefix (applies to all its wrapped rows).
fn line_style(l: &str) -> Style {
    if l.starts_with("[err]") {
        Style::default().fg(RED)
    } else if l.starts_with("> ") {
        Style::default().fg(CYAN)
    } else if l.starts_with("$ ") {
        Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
    } else if l.starts_with("──") || l.starts_with("::") {
        Style::default().fg(DIM).add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(FG)
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let content = match app.mode {
        Mode::Input | Mode::HashInput | Mode::StdinInput | Mode::Filter | Mode::Search => {
            let (label, help) = match app.mode {
                Mode::HashInput => (" hash ▸ ", "  Enter identify · Esc cancel"),
                Mode::StdinInput => (" stdin ▸ ", "  Enter send · Ctrl+D end input · Esc close"),
                Mode::Filter => (
                    " filter ▸ ",
                    "  name / category / text · Enter keep · Esc clear",
                ),
                Mode::Search => (
                    " find ▸ ",
                    "  Enter find in output · then n / N · Esc cancel",
                ),
                _ => (" args ▸ ", "  Enter run · ↑↓ history · Esc cancel"),
            };
            let label_w = label.chars().count() as u16 + 1; // + the space after it
                                                            // Drop the help text when space is tight; the line itself matters more.
            let help_w = help.chars().count() as u16;
            let show_help = area.width > label_w + help_w + 20;
            let text_w = area
                .width
                .saturating_sub(label_w + if show_help { help_w } else { 0 })
                .max(1);
            let (visible, col) = app.editor.view(text_w as usize);
            f.set_cursor_position((area.x + label_w + col as u16, area.y));
            let mut spans = vec![
                Span::styled(
                    label,
                    Style::default()
                        .fg(INK)
                        .bg(YELLOW)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{visible:<w$}", w = text_w as usize),
                    Style::default().fg(FG),
                ),
            ];
            if show_help {
                spans.push(Span::styled(help, Style::default().fg(DIM)));
            }
            Line::from(spans)
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
            Span::styled("/", Style::default().fg(CYAN)),
            Span::styled(" filter · ", Style::default().fg(DIM)),
            Span::styled("f", Style::default().fg(CYAN)),
            Span::styled(" find · ", Style::default().fg(DIM)),
            Span::styled("Enter", Style::default().fg(CYAN)),
            Span::styled(" run · ", Style::default().fg(DIM)),
            Span::styled("i", Style::default().fg(CYAN)),
            Span::styled(" install · ", Style::default().fg(DIM)),
            Span::styled("r", Style::default().fg(CYAN)),
            Span::styled(" re-detect · ", Style::default().fg(DIM)),
            Span::styled("c", Style::default().fg(CYAN)),
            Span::styled(" cancel · ", Style::default().fg(DIM)),
            Span::styled("h", Style::default().fg(CYAN)),
            Span::styled(" hash id · ", Style::default().fg(DIM)),
            Span::styled("q", Style::default().fg(CYAN)),
            Span::styled(" quit", Style::default().fg(DIM)),
        ]),
    };
    f.render_widget(Paragraph::new(content), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn app() -> App {
        let cat = registry::parse(include_str!("../tools.yaml")).unwrap();
        App::new(cat, PathBuf::from("tools.yaml"), None)
    }

    fn draw(app: &App, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| render(f, app)).unwrap();
        t
    }

    fn row(t: &Terminal<TestBackend>, y: u16) -> String {
        let buf = t.backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn renders_at_common_and_tiny_sizes() {
        let a = app();
        for (w, h) in [(120, 40), (80, 24), (40, 10), (10, 4)] {
            draw(&a, w, h);
        }
    }

    #[test]
    fn args_prompt_shows_text_and_places_cursor() {
        let mut a = app();
        a.mode = Mode::Input;
        a.editor.set("-sV scanme.nmap.org");
        a.editor.home();
        let mut t = draw(&a, 100, 20);
        let footer = row(&t, 19);
        assert!(footer.contains("args ▸"), "{footer}");
        assert!(footer.contains("-sV scanme.nmap.org"), "{footer}");
        // cursor sits on the first character of the text
        let label_w = " args ▸ ".chars().count() as u16 + 1;
        assert_eq!(t.get_cursor_position().unwrap(), (label_w, 19).into());
    }

    #[test]
    fn long_args_scroll_to_keep_the_cursor_visible() {
        let mut a = app();
        a.mode = Mode::Input;
        let long = format!("-u https://example.com/{}END", "x".repeat(200));
        a.editor.set(&long);
        let mut t = draw(&a, 60, 12);
        let footer = row(&t, 11);
        assert!(footer.contains("END"), "{footer}");
        assert!(t.get_cursor_position().unwrap().x < 60);
    }

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn tail_accounts_for_wrapped_rows() {
        // the last line is 2.5x the width: the newest rows must be visible
        let out = lines(&["one", "two", &"x".repeat(25)]);
        let (rows, max) = visible_rows(&out, 10, 3, 0);
        let text: Vec<&str> = rows.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(text, ["xxxxxxxxxx", "xxxxxxxxxx", "xxxxx"]);
        assert_eq!(max, 2); // 5 rows total, 3 visible
    }

    #[test]
    fn scrolling_up_and_clamping() {
        let out = lines(&["a", "b", "c", "d", "e"]);
        let at = |s| {
            visible_rows(&out, 10, 2, s)
                .0
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>()
        };
        assert_eq!(at(0), ["d", "e"]);
        assert_eq!(at(1), ["c", "d"]);
        assert_eq!(at(3), ["a", "b"]);
        assert_eq!(at(99), ["a", "b"]); // clamped to the top
        assert_eq!(visible_rows(&out, 10, 2, 0).1, 3);
    }

    #[test]
    fn empty_lines_and_indices() {
        let out = lines(&["", "abcdef"]);
        let (rows, _) = visible_rows(&out, 4, 5, 0);
        assert_eq!(
            rows,
            [
                (0, "".to_string()),
                (1, "abcd".to_string()),
                (1, "ef".to_string())
            ]
        );
    }

    #[test]
    fn filter_narrows_the_list() {
        let mut a = app();
        a.filter = "reversing".into();
        let vis = a.visible();
        assert!(vis.len() > 3);
        assert!(vis.iter().all(|&i| a.tools[i].category == "reversing"));
        // words AND together, across name/category/description
        a.filter = "web brute".into();
        let names: Vec<&str> = a
            .visible()
            .iter()
            .map(|&i| a.tools[i].name.as_str())
            .collect();
        assert!(names.contains(&"dirsearch"), "{names:?}");
        assert!(!names.contains(&"nmap"));
        // renders with a filter, including one that matches nothing
        draw(&a, 100, 20);
        a.filter = "zzz-no-match".into();
        assert!(a.visible().is_empty());
        draw(&a, 100, 20);
    }

    #[test]
    fn hash_prompt_label() {
        let mut a = app();
        a.mode = Mode::HashInput;
        let t = draw(&a, 100, 20);
        assert!(row(&t, 19).contains("hash ▸"));
    }
}
