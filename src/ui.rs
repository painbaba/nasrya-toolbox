//! Main dashboard rendering. Every colour, glyph and border comes from the
//! active `Theme` (see theme.rs), so the `t` overlay can restyle it live.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, Appearance, Mode, ToolStatus};
use crate::banner;
use crate::theme::{Backdrop, Field, OverlayPos, Theme, ThemeName};

pub fn render(f: &mut Frame, app: &App) {
    let th = app.settings.theme();
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),    // body
            Constraint::Length(1), // footer / input
        ])
        .split(area);

    render_header(f, app, &th, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(chunks[1]);

    render_tool_list(f, app, &th, body[0]);
    render_output(f, app, &th, body[1]);
    render_footer(f, app, &th, chunks[2]);
    if let Some(ap) = &app.appearance {
        render_appearance(f, app, ap, &th, area);
    } else if app.show_help {
        render_help(f, &th, area);
    }
}

/// Every key binding, shown by `?`.
pub const HELP: &[(&str, &str)] = &[
    ("↑ ↓  j k", "move in the tool list"),
    ("/", "filter tools by name / category / description"),
    (
        "Enter",
        "run the tool (edit args first) · on a dataset: list it",
    ),
    (
        "Enter (running)",
        "send a line to the tool's input · Ctrl+D = EOF",
    ),
    ("i", "install the selected tool (scoop / pipx)"),
    ("c", "cancel the running process (whole tree)"),
    ("r", "re-detect installed tools (picks up PATH changes)"),
    ("PgUp PgDn", "scroll the output · Home top · End follow"),
    ("f  n  N", "find in the output · older / newer match"),
    ("h", "identify a hash (built in)"),
    ("t", "appearance: theme, borders, glyphs, overlays"),
    ("?", "this help"),
    ("q  Esc", "quit"),
    ("", ""),
    (
        "in prompts",
        "← → Home End · Ctrl+← → words · Ctrl+W / Ctrl+U delete · ↑ ↓ history",
    ),
];

// ---------------------------------------------------------------- overlays

/// Where an overlay of `w` x `h` goes: centered, or docked full-width just
/// above the footer the way Claude Code shows its pickers. Also fades the
/// dashboard behind it when the backdrop is set to dim.
fn overlay_area(f: &mut Frame, th: &Theme, area: Rect, w: u16, h: u16) -> Rect {
    let h = h.min(area.height);
    let popup = match th.overlay {
        OverlayPos::Centered => {
            let w = w.min(area.width);
            Rect {
                x: area.x + area.width.saturating_sub(w) / 2,
                y: area.y + area.height.saturating_sub(h) / 2,
                width: w,
                height: h,
            }
        }
        OverlayPos::Docked => {
            // keep the footer row visible; leave the header if there's room
            let h = h.min(area.height.saturating_sub(1).max(1));
            Rect {
                x: area.x,
                y: (area.y + area.height).saturating_sub(1 + h).max(area.y),
                width: area.width,
                height: h,
            }
        }
    };
    if th.backdrop == Backdrop::Dim {
        let buf = f.buffer_mut();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                cell.set_fg(th.pal.dim);
                cell.set_bg(Color::Reset);
                cell.modifier = Modifier::DIM;
            }
        }
    }
    f.render_widget(Clear, popup);
    popup
}

/// The frame every overlay shares: accent border, bold title, hint bottom right.
fn overlay_block<'a>(th: &Theme, title: &'a str, hint: &'a str) -> Block<'a> {
    th.block(th.pal.accent)
        .title(Span::styled(format!(" {title} "), th.strong(th.pal.accent)))
        .title_bottom(
            Line::from(Span::styled(format!(" {hint} "), th.fg(th.pal.dim)))
                .alignment(Alignment::Right),
        )
}

fn render_help(f: &mut Frame, th: &Theme, area: Rect) {
    let p = &th.pal;
    let key_w = HELP
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {k:<key_w$}  "), th.strong(p.accent)),
                Span::styled(d.to_string(), th.fg(p.fg)),
            ])
        })
        .collect();
    let w = lines.iter().map(|l| l.width()).max().unwrap_or(40) as u16 + 3;
    let h = lines.len() as u16 + 2;
    let popup = overlay_area(f, th, area, w, h);
    f.render_widget(
        Paragraph::new(lines).block(overlay_block(th, "keys", "any key to close")),
        popup,
    );
}

/// The `t` overlay: settings on the left, a live sample of the dashboard
/// on the right (stacked when narrow). Everything, including this panel,
/// is already drawn with the previewed settings.
fn render_appearance(f: &mut Frame, app: &App, ap: &Appearance, th: &Theme, area: Rect) {
    let p = &th.pal;
    let s = &app.settings;
    let g = th.glyph;

    let label_w = Field::ALL
        .iter()
        .map(|x| x.label().chars().count())
        .max()
        .unwrap_or(0);
    let mut rows: Vec<Line> = vec![Line::raw("")];
    for (i, field) in Field::ALL.iter().enumerate() {
        let on = i == ap.cursor;
        let cursor = if on { g.pointer } else { "  " };
        let value = s.value(*field);
        let mut spans = vec![
            Span::styled(format!(" {cursor}"), th.fg(p.accent)),
            Span::styled(
                format!("{:<label_w$}  ", field.label()),
                if on { th.strong(p.fg) } else { th.fg(p.dim) },
            ),
        ];
        if on {
            spans.push(Span::styled(format!("{} ", g.left), th.fg(p.accent)));
            spans.push(Span::styled(value, th.strong(p.accent)));
            spans.push(Span::styled(format!(" {}", g.right), th.fg(p.accent)));
        } else {
            spans.push(Span::styled(format!("  {value}"), th.fg(p.fg)));
        }
        rows.push(Line::from(spans));
    }
    rows.push(Line::raw(""));
    // the theme list, numbered like Claude Code's /theme
    for (i, t) in ThemeName::ALL.iter().enumerate() {
        let cur = *t == s.theme;
        rows.push(Line::from(vec![
            Span::styled(
                format!("   {} ", i + 1),
                if cur {
                    th.strong(p.accent)
                } else {
                    th.fg(p.dim)
                },
            ),
            Span::styled(
                format!("{}{}", t.label(), if cur { "  ✓" } else { "" }),
                if cur { th.fg(p.fg) } else { th.fg(p.dim) },
            ),
        ]));
    }

    let preview = preview_lines(th);
    let left_w = rows.iter().map(|l| l.width()).max().unwrap_or(30) as u16 + 2;
    let right_w = preview.iter().map(|l| l.width()).max().unwrap_or(30) as u16 + 4;
    let side_by_side = area.width >= left_w + right_w + 4;
    let w = if side_by_side {
        left_w + right_w + 3
    } else {
        left_w.max(right_w) + 2
    };
    let h = if side_by_side {
        rows.len().max(preview.len() + 2) as u16 + 2
    } else {
        (rows.len() + preview.len() + 2) as u16 + 2
    };
    let popup = overlay_area(f, th, area, w, h);
    let block = overlay_block(
        th,
        "appearance",
        "↑↓ choose · ←→ change · 1-7 theme · Enter save · Esc cancel",
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let parts = Layout::default()
        .direction(if side_by_side {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([
            Constraint::Length(if side_by_side {
                left_w
            } else {
                rows.len() as u16
            }),
            Constraint::Min(0),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(rows), parts[0]);

    let sample = th
        .block(p.border)
        .title(Span::styled(" preview ", th.fg(p.dim)));
    let sample_area = if side_by_side {
        Rect {
            x: parts[1].x + 1,
            y: parts[1].y + 1,
            width: right_w.min(parts[1].width.saturating_sub(2)),
            height: (preview.len() as u16 + 2).min(parts[1].height.saturating_sub(1)),
        }
    } else {
        parts[1]
    };
    f.render_widget(Paragraph::new(preview).block(sample), sample_area);
}

/// A few rows that exercise every palette slot.
fn preview_lines(th: &Theme) -> Vec<Line<'static>> {
    let p = &th.pal;
    let g = th.glyph;
    let tool = |icon: &'static str, c, name: &'static str, cat: &'static str| {
        Line::from(vec![
            Span::styled(format!("{icon} "), th.fg(c)),
            Span::styled(format!("{name:<9}"), th.strong(p.fg)),
            Span::styled(format!(" {cat}"), th.fg(p.dim)),
        ])
    };
    vec![
        Line::from(vec![
            Span::styled(g.pointer, th.emph(Style::default().fg(p.ink).bg(p.accent))),
            Span::styled(
                "nmap      recon    ",
                th.emph(Style::default().fg(p.ink).bg(p.accent)),
            ),
        ]),
        tool(g.installed, p.ok, "ffuf", "web"),
        tool(g.missing, p.err, "sqlmap", "web"),
        tool(g.conflict, p.warn, "john", "cracking"),
        Line::raw(""),
        Line::from(Span::styled(
            "$ nmap -sV scanme.nmap.org",
            line_style(th, "$ "),
        )),
        Line::from(Span::styled("22/tcp open  ssh", th.fg(p.fg))),
        Line::from(vec![
            Span::styled("80/tcp ", th.fg(p.fg)),
            Span::styled("open", Style::default().fg(p.fg).bg(p.hit_bg)),
            Span::styled("  http", th.fg(p.fg)),
        ]),
        Line::from(Span::styled("> y", line_style(th, "> "))),
        Line::from(Span::styled(
            "[err] host seems down",
            line_style(th, "[err]"),
        )),
        Line::from(Span::styled(":: exit 0 · 4.2s", line_style(th, "::"))),
        Line::from(vec![
            Span::styled(format!("{} running ", g.spinner[0]), th.strong(p.warn)),
            Span::styled("pid 4242", th.fg(p.dim)),
        ]),
    ]
}

// --------------------------------------------------------------- dashboard

fn render_header(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(28)])
        .split(area);

    f.render_widget(Paragraph::new(banner::wordmark(th)), cols[0]);

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
        Span::styled(th.glyph.installed, th.fg(p.ok)),
        Span::styled(format!(" {installed} "), th.fg(p.fg)),
        Span::styled(th.glyph.missing, th.fg(p.err)),
        Span::styled(format!(" {missing} "), th.fg(p.fg)),
        Span::styled(format!("· {} tools ", app.tools.len()), th.fg(p.dim)),
    ])
    .alignment(Alignment::Right);
    f.render_widget(Paragraph::new(stats), cols[1]);
}

fn render_tool_list(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let g = th.glyph;
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
                ToolStatus::Installed => (g.installed, p.ok),
                ToolStatus::Missing => (g.missing, p.err),
                ToolStatus::Unknown => (g.unknown, p.dim),
                ToolStatus::Conflict => (g.conflict, p.warn),
            };
            let line = Line::from(vec![
                Span::styled(format!("{icon} "), th.fg(color)),
                Span::styled(format!("{:<name_w$}", t.name), th.strong(p.fg)),
                Span::styled(format!(" {}", t.category), th.fg(p.dim)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let sel_pos = visible.iter().position(|&i| i == app.selected);
    let pos = match sel_pos {
        Some(i) => format!(" {}/{} ", i + 1, visible.len()),
        None => format!(" 0/{} ", visible.len()),
    };
    let mut title = vec![Span::styled(" tools ", th.strong(p.accent))];
    if !app.filter.trim().is_empty() {
        title.push(Span::styled(
            format!(
                "/ {} ({} of {}) ",
                app.filter.trim(),
                visible.len(),
                app.tools.len()
            ),
            th.fg(p.warn),
        ));
    }
    let list =
        List::new(items)
            .block(th.block(p.border).title(Line::from(title)).title_bottom(
                Line::from(Span::styled(pos, th.fg(p.dim))).alignment(Alignment::Right),
            ))
            .highlight_style(th.emph(Style::default().bg(p.accent).fg(p.ink)))
            .highlight_symbol(g.pointer);

    let mut state = ListState::default();
    state.select(sel_pos);
    f.render_stateful_widget(list, area, &mut state);
}

fn render_output(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let tool = &app.tools[app.selected];

    let title = Line::from(vec![
        Span::styled(format!(" {} ", tool.name), th.strong(p.accent)),
        Span::styled(format!("· {} ", tool.description), th.fg(p.dim)),
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
                th.fg(p.dim).add_modifier(Modifier::ITALIC),
            )),
            Line::from(Span::styled(
                "press Enter to run · i to install".to_string(),
                th.fg(p.border),
            )),
        ]
    } else {
        rows.into_iter()
            .map(|(i, chunk)| {
                let mut style = line_style(th, &app.output[i]);
                if app.search_hit == Some(i) {
                    style = th.emph(style.bg(p.hit_bg));
                }
                Line::from(Span::styled(chunk, style))
            })
            .collect()
    };

    let mut bottom = if app.running {
        let spin = th.glyph.spinner;
        let frame = spin[(app.tick / 2 % spin.len() as u64) as usize];
        let pid = app
            .running_pid
            .map(|p| format!("pid {p} "))
            .unwrap_or_default();
        Line::from(vec![
            Span::styled(format!(" {frame} running "), th.strong(p.warn)),
            Span::styled(pid, th.fg(p.dim)),
        ])
    } else {
        Line::from(Span::styled(" idle ", th.fg(p.dim)))
    };
    if app.scroll > 0 {
        bottom.push_span(Span::styled(
            format!(" ↑ {} rows up · End to follow ", app.scroll.min(max_scroll)),
            th.fg(p.info),
        ));
    }

    let border_color = if app.running { p.warn } else { p.border };

    let para = Paragraph::new(text).block(th.block(border_color).title(title).title_bottom(bottom));
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
fn line_style(th: &Theme, l: &str) -> Style {
    let p = &th.pal;
    if l.starts_with("[err]") {
        th.fg(p.err)
    } else if l.starts_with("> ") {
        th.fg(p.info)
    } else if l.starts_with("$ ") {
        th.strong(p.warn)
    } else if l.starts_with("──") || l.starts_with("::") {
        th.fg(p.dim).add_modifier(Modifier::ITALIC)
    } else {
        th.fg(p.fg)
    }
}

fn render_footer(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let arrow = th.glyph.arrow;
    let content = match app.mode {
        Mode::Input | Mode::HashInput | Mode::StdinInput | Mode::Filter | Mode::Search => {
            let (name, help) = match app.mode {
                Mode::HashInput => ("hash", "  Enter identify · Esc cancel"),
                Mode::StdinInput => ("stdin", "  Enter send · Ctrl+D end input · Esc close"),
                Mode::Filter => (
                    "filter",
                    "  name / category / text · Enter keep · Esc clear",
                ),
                Mode::Search => ("find", "  Enter find in output · then n / N · Esc cancel"),
                _ => ("args", "  Enter run · ↑↓ history · Esc cancel"),
            };
            let label = format!(" {name} {arrow} ");
            let label_w = label.chars().count() as u16 + 1; // + the space after it
                                                            // Drop the help text when space is tight; the line itself matters more.
            let help_w = help.chars().count() as u16;
            let show_help = area.width > label_w + help_w + 20;
            let text_w = area
                .width
                .saturating_sub(label_w + if show_help { help_w } else { 0 })
                .max(1);
            let (visible, col) = app.editor.view(text_w as usize);
            // no terminal cursor while an overlay sits on top
            if app.appearance.is_none() && !app.show_help {
                f.set_cursor_position((area.x + label_w + col as u16, area.y));
            }
            let mut spans = vec![
                Span::styled(label, th.emph(Style::default().fg(p.ink).bg(p.warn))),
                Span::raw(" "),
                Span::styled(format!("{visible:<w$}", w = text_w as usize), th.fg(p.fg)),
            ];
            if show_help {
                spans.push(Span::styled(help, th.fg(p.dim)));
            }
            Line::from(spans)
        }
        Mode::Browsing => {
            let key = |k: &'static str| Span::styled(k, th.fg(p.accent));
            let sep = |d: &'static str| Span::styled(d, th.fg(p.dim));
            Line::from(vec![
                Span::styled(
                    format!(" {} ", app.status_line),
                    th.emph(Style::default().fg(p.ink).bg(p.accent)),
                ),
                Span::raw("  "),
                key("↑/↓"),
                sep(" move · "),
                key("Enter"),
                sep(" run · "),
                key("i"),
                sep(" install · "),
                key("/"),
                sep(" filter · "),
                key("c"),
                sep(" cancel · "),
                key("t"),
                sep(" theme · "),
                Span::styled("?", th.strong(p.accent)),
                sep(" all keys · "),
                key("q"),
                sep(" quit"),
            ])
        }
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
    fn help_overlay_renders_and_lists_every_browsing_key() {
        let mut a = app();
        a.show_help = true;
        for (w, h) in [(100, 30), (40, 10)] {
            draw(&a, w, h);
        }
        let t = draw(&a, 120, 30);
        let screen: String = (0..30).map(|y| row(&t, y)).collect::<Vec<_>>().join("\n");
        for k in [
            "filter",
            "install",
            "find in the output",
            "identify a hash",
            "any key to close",
        ] {
            assert!(screen.contains(k), "help is missing {k:?}");
        }
    }

    #[test]
    fn hash_prompt_label() {
        let mut a = app();
        a.mode = Mode::HashInput;
        let t = draw(&a, 100, 20);
        assert!(row(&t, 19).contains("hash ▸"));
    }

    #[test]
    fn appearance_overlay_renders_every_theme_and_layout() {
        use crate::app::Appearance;
        use crate::theme::{Backdrop, Borders, GlyphSet, OverlayPos, ThemeName};
        let mut a = app();
        a.appearance = Some(Appearance {
            cursor: 0,
            saved: a.settings,
        });
        for theme in ThemeName::ALL {
            for overlay in [OverlayPos::Docked, OverlayPos::Centered] {
                a.settings.theme = theme;
                a.settings.overlay = overlay;
                for (w, h) in [(140, 40), (80, 24), (40, 10), (10, 4)] {
                    draw(&a, w, h);
                }
            }
        }
        a.settings.theme = ThemeName::Dark;
        a.settings.overlay = OverlayPos::Centered;
        let t = draw(&a, 140, 40);
        let screen: String = (0..40).map(|y| row(&t, y)).collect::<Vec<_>>().join(
            "
",
        );
        for k in [
            "appearance",
            "preview",
            "Dark mode",
            "Borders",
            "Enter save",
        ] {
            assert!(screen.contains(k), "appearance overlay is missing {k:?}");
        }
        // ascii everything: no box-drawing or braille left in the dashboard
        a.appearance = None;
        a.settings.borders = Borders::Ascii;
        a.settings.glyphs = GlyphSet::Ascii;
        a.settings.backdrop = Backdrop::None;
        let t = draw(&a, 100, 20);
        let screen: String = (0..20).map(|y| row(&t, y)).collect::<Vec<_>>().join(
            "
",
        );
        for bad in ["╭", "─", "│", "●", "○", "▸"] {
            assert!(!screen.contains(bad), "ascii mode still draws {bad:?}");
        }
        assert!(screen.contains("+--"), "{screen}");
    }

    #[test]
    fn docked_overlay_keeps_the_footer_and_dims_the_rest() {
        let mut a = app();
        a.show_help = true;
        let t = draw(&a, 120, 40);
        let buf = t.backend().buffer();
        // the footer row is still the footer
        assert!(row(&t, 39).contains("quit"), "{}", row(&t, 39));
        // the header behind the overlay is faded
        assert!(buf[(1, 0)].modifier.contains(Modifier::DIM));
    }
}
