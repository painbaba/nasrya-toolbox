//! Main dashboard rendering. Every colour, glyph and border comes from the
//! active `Theme` (see theme.rs), so the `t` overlay can restyle it live.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, Appearance, CatTab, Mode, ToastKind, ToolStatus, TOUR_STEPS};
use crate::banner;
use crate::registry::{self, Launch};
use crate::session::{RunState, Session, SLOTS};
use crate::theme::{Backdrop, Field, OverlayPos, Theme, ThemeName};

pub fn render(f: &mut Frame, app: &App) {
    let mut th = app.settings.theme();
    th.mono = app.mono;
    let th = th;
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // category bar, full width
            Constraint::Min(3),    // body
            Constraint::Length(1), // footer / input
        ])
        .split(area);

    render_header(f, app, &th, chunks[0]);
    render_categories(f, app, &th, chunks[1]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(chunks[2]);

    // left: tool list, card for the selected tool
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(if body[0].height >= 15 { 6 } else { 0 }),
        ])
        .split(body[0]);
    render_tool_list(f, app, &th, left[0]);
    render_tool_card(f, app, &th, left[1]);

    // right: terminal tabs, then one terminal or the 2x2 grid
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3)])
        .split(body[1]);
    render_terminal_tabs(f, app, &th, right[0]);
    render_terminals(f, app, &th, right[1]);
    render_footer(f, app, &th, chunks[3]);
    // toasts sit under the overlays, the tour over everything
    render_toasts(f, app, &th, chunks[2]);
    if let Some(ap) = &app.appearance {
        render_appearance(f, app, ap, &th, area);
    } else if app.show_help {
        render_help(f, &th, area);
    }
    if let Some(step) = app.tour {
        render_tour(f, &th, area, step);
    }
}

/// Every key binding, shown by `?`.
pub const HELP: &[(&str, &str)] = &[
    ("↑ ↓  j k", "move in the tool list"),
    (
        "← →  [ ]",
        "switch category: All · Installed · OSINT … Wordlists",
    ),
    ("/", "filter tools by name / category / description"),
    (
        "Enter",
        "run the tool in a free terminal (edit args first) · dataset: list it",
    ),
    (
        "0-4  Tab",
        "show the console (0) or terminal 1-4 · Tab cycles",
    ),
    ("g", "grid: all four terminals at once"),
    ("s", "send a line to the shown tool's input · Ctrl+D = EOF"),
    ("c", "stop the shown tool (its output stays)"),
    (
        "x",
        "close the shown terminal (twice if its tool is running)",
    ),
    (
        "i",
        "install the selected tool (scoop / pipx), in the console",
    ),
    ("r", "re-detect installed tools (picks up PATH changes)"),
    ("PgUp PgDn", "scroll the terminal · Home top · End follow"),
    ("f  n  N", "find in the output · older / newer match"),
    ("h", "identify a hash (built in)"),
    ("t", "appearance: theme, borders, glyphs, overlays"),
    ("?", "this help"),
    ("q  Esc", "quit (twice if tools are still running)"),
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
        OverlayPos::Centered => centered(area, w, h),
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
    dim_backdrop(f, th, area);
    f.render_widget(Clear, popup);
    popup
}

/// A `w` x `h` box in the middle of `area` (clamped to fit).
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// Fade everything already drawn, if the backdrop setting asks for it.
fn dim_backdrop(f: &mut Frame, th: &Theme, area: Rect) {
    if th.backdrop != Backdrop::Dim {
        return;
    }
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

/// A solid label (prompt names, a pending confirm). Reverse video when
/// colours are stripped, or the background would vanish.
fn pill(th: &Theme, bg: Color) -> Style {
    if th.mono {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        th.emph(Style::default().fg(th.pal.ink).bg(bg))
    }
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
            Span::styled(g.pointer, th.selected()),
            Span::styled("nmap      recon    ", th.selected()),
        ]),
        tool(g.installed, p.ok, "ffuf", "web"),
        tool(g.missing, p.dim, "sqlmap", "web"),
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
        .constraints([Constraint::Min(10), Constraint::Length(52)])
        .split(area);

    f.render_widget(Paragraph::new(banner::wordmark(th)), cols[0]);

    let count = |st: ToolStatus| app.status.iter().filter(|s| **s == st).count();
    let installed = count(ToolStatus::Installed);
    let missing = count(ToolStatus::Missing);

    let mut stats = Vec::new();
    let running = app.running_count();
    if running > 0 {
        let spin = th.glyph.spinner;
        let frame = spin[(app.tick / 2 % spin.len() as u64) as usize];
        stats.push(Span::styled(
            format!("{frame} {running} running  "),
            th.strong(p.warn),
        ));
    }
    // labelled, so nobody has to guess what the dots mean
    stats.extend([
        Span::styled(th.glyph.installed, th.fg(p.ok)),
        Span::styled(format!(" {installed} ready  "), th.fg(p.fg)),
        Span::styled(th.glyph.missing, th.fg(p.dim)),
        Span::styled(format!(" {missing} to get "), th.fg(p.fg)),
        Span::styled(format!("· {} tools ", app.tools.len()), th.fg(p.dim)),
    ]);
    let stats = Line::from(stats).alignment(Alignment::Right);
    f.render_widget(Paragraph::new(stats), cols[1]);
}

/// The monochrome symbol in front of a category chip (unicode glyph set
/// only): one cell wide, coloured like its label.
fn cat_icon(tab: &CatTab) -> &'static str {
    match tab {
        CatTab::All => "≡",
        CatTab::Installed => "✓",
        CatTab::Cat(id) => match id.as_str() {
            "osint" => "◎",
            "recon" => "⌖",
            "network" => "⇄",
            "web" => "⊕",
            "exploit" => "↯",
            "passwords" => "⊛",
            "reversing" => "⟲",
            "pwn" => "▲",
            "mobile" => "▯",
            "forensics" => "⌕",
            "crypto" => "⌗",
            "stego" => "◈",
            "wordlists" => "▤",
            _ => "◇",
        },
    }
}

/// The category bar: a full-width row of chips with how many tools each
/// shows (filter applied), scrolled so the active one is in view, with
/// markers when chips are hidden off either edge.
fn render_categories(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let g = th.glyph;
    let sep = if g.icons { " │ " } else { " | " };
    let chips: Vec<Line> = app
        .cat_tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let n = app.visible_in(t).len();
            let icon = if g.icons {
                format!("{} ", cat_icon(t))
            } else {
                String::new()
            };
            if i == app.cat {
                // text, not a bar: the solid bar belongs to the selected tool
                let on = th.strong(p.accent).add_modifier(Modifier::UNDERLINED);
                Line::from(Span::styled(format!("{icon}{} {n}", t.label()), on))
            } else {
                let name = if n == 0 { th.fg(p.dim) } else { th.fg(p.fg) };
                Line::from(vec![
                    Span::styled(format!("{icon}{}", t.label()), name),
                    Span::styled(format!(" {n}"), th.fg(p.dim)),
                ])
            }
        })
        .collect();
    if chips.is_empty() {
        return;
    }
    let w: Vec<usize> = chips.iter().map(Line::width).collect();
    let sep_w = sep.chars().count();
    let run = |a: usize, b: usize| w[a..=b].iter().sum::<usize>() + sep_w * (b - a);
    // two columns on each side for the overflow markers
    let avail = (area.width as usize).saturating_sub(4);
    let active = app.cat.min(chips.len() - 1);
    let mut start = 0;
    while start < active && run(start, active) > avail {
        start += 1;
    }
    let mut end = active.max(start);
    while end + 1 < chips.len() && run(start, end + 1) <= avail {
        end += 1;
    }

    let mut spans = vec![Span::styled(
        if start > 0 {
            format!("{} ", g.more_l)
        } else {
            "  ".to_string()
        },
        th.fg(p.dim),
    )];
    for (i, chip) in chips.iter().enumerate().take(end + 1).skip(start) {
        if i > start {
            spans.push(Span::styled(sep, th.fg(p.dim)));
        }
        spans.extend(chip.spans.iter().cloned());
    }
    if end + 1 < chips.len() {
        spans.push(Span::styled(format!(" {}", g.more_r), th.fg(p.dim)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Icon, its colour, and the short badge shown at the end of a list row.
fn status_look(th: &Theme, st: ToolStatus) -> (&'static str, Color, &'static str) {
    let p = &th.pal;
    let g = th.glyph;
    let fancy = g.icons;
    match st {
        ToolStatus::Installed => (g.installed, p.ok, if fancy { "✓ ready" } else { "ready" }),
        // not installed is an offer, not an error: no red
        ToolStatus::Missing => (g.missing, p.dim, "+ get"),
        ToolStatus::Unknown => (g.unknown, p.dim, if fancy { "…" } else { "..." }),
        ToolStatus::Conflict => (g.conflict, p.warn, "! PATH"),
    }
}

/// `s` cut to `max` columns, with an ellipsis when something was dropped.
fn clip(s: &str, max: usize, fancy: bool) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if !fancy {
        return s.chars().take(max).collect();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    if max > 0 {
        out.push('…');
    }
    out
}

fn render_tool_list(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let g = th.glyph;
    let tab = &app.cat_tabs[app.cat];
    // Pad names to the longest one so the category column stays aligned.
    let name_w = app
        .tools
        .iter()
        .map(|t| t.name.chars().count())
        .max()
        .unwrap_or(0);
    let glow_w = g.glow_l.chars().count();
    let content_w = (area.width.saturating_sub(2) as usize).saturating_sub(glow_w * 2);
    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let t = &app.tools[i];
            let st = app.status[i];
            let (icon, color, badge) = status_look(th, st);
            // its other categories (the one being browsed goes without saying)
            let others: Vec<&str> = t
                .categories
                .iter()
                .map(String::as_str)
                .filter(|c| !matches!(tab, CatTab::Cat(id) if id == c))
                .collect();
            let name = format!("{:<name_w$}", t.name);
            let tail = if i == app.selected {
                match st {
                    ToolStatus::Installed if g.icons => "⏎ run",
                    ToolStatus::Installed => "Enter run",
                    ToolStatus::Conflict => "! PATH",
                    _ => "i install",
                }
            } else {
                badge
            };
            // "▸ ● name " before it, " badge " after it
            let room = content_w.saturating_sub(5 + name_w + tail.chars().count() + 2);
            let others = clip(&others.join(" "), room, g.icons);

            if i == app.selected {
                // the glow bar: one solid "you are here", with what Enter does
                let hint = tail;
                let left = format!("{} {icon} {name} {others}", g.arrow);
                let used = Span::raw(left.as_str()).width() + hint.chars().count() + 1;
                let pad = " ".repeat(content_w.saturating_sub(used).max(1));
                return ListItem::new(Line::from(vec![
                    Span::styled(g.glow_l, th.fg(p.accent)),
                    Span::styled(format!("{left}{pad}{hint} "), th.selected()),
                    Span::styled(g.glow_r, th.fg(p.accent)),
                ]));
            }

            let left_w = Span::raw(format!("  {icon} {name} {others}")).width();
            let used = left_w + badge.chars().count() + 1;
            let pad = " ".repeat(content_w.saturating_sub(used).max(1));
            ListItem::new(Line::from(vec![
                Span::raw(" ".repeat(glow_w + 2)),
                Span::styled(icon, th.fg(color)),
                Span::raw(" "),
                Span::styled(name, th.strong(p.fg)),
                Span::styled(format!(" {others}"), th.fg(p.dim)),
                Span::raw(pad),
                Span::styled(badge, th.fg(color)),
            ]))
        })
        .collect();

    let sel_pos = visible.iter().position(|&i| i == app.selected);
    let pos = match sel_pos {
        Some(i) => format!(" {}/{} ", i + 1, visible.len()),
        None => format!(" 0/{} ", visible.len()),
    };
    let mut title = vec![Span::styled(
        format!(" {} ", tab.label()),
        th.strong(p.accent),
    )];
    if !app.filter.trim().is_empty() {
        title.push(Span::styled(
            format!("/ {} ", app.filter.trim()),
            th.fg(p.warn),
        ));
    }
    let block = th
        .block(p.border)
        .title(Line::from(title))
        .title_bottom(Line::from(Span::styled(pos, th.fg(p.dim))).alignment(Alignment::Right));

    if items.is_empty() {
        // what happened and where to look, instead of an empty box
        let elsewhere = app.visible_in(&CatTab::All).len();
        let filter = app.filter.trim();
        let face = if g.icons { "∅" } else { "(none)" };
        let what = if !filter.is_empty() {
            format!("nothing for \"{filter}\" in {}", tab.label())
        } else if matches!(tab, CatTab::Installed) {
            "nothing installed yet".to_string()
        } else {
            format!("nothing in {} yet", tab.label())
        };
        let mut lines = vec![
            Line::raw(""),
            Line::from(Span::styled(face, th.strong(p.accent))),
            Line::raw(""),
            Line::from(Span::styled(what, th.fg(p.fg))),
        ];
        if !filter.is_empty() && elsewhere > 0 {
            lines.push(Line::from(Span::styled(
                format!("{elsewhere} match(es) in All · ← → to look · Esc clears"),
                th.fg(p.info),
            )));
        } else if matches!(tab, CatTab::Installed) {
            lines.push(Line::from(Span::styled(
                "pick a tool and press i to install it",
                th.fg(p.info),
            )));
        }
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .block(block),
            area,
        );
        return;
    }

    // the selected row draws its own bar, so no highlight style or symbol
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    state.select(sel_pos);
    f.render_stateful_widget(list, area, &mut state);
}

/// What the highlighted tool is: its categories (main one first), whether
/// it's installed, how it runs, and what it does.
fn render_tool_card(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    if area.height == 0 {
        return;
    }
    let p = &th.pal;
    let g = th.glyph;
    let t = &app.tools[app.selected];
    let mut cats = Vec::new();
    for (k, c) in t.categories.iter().enumerate() {
        if k > 0 {
            cats.push(Span::styled(" · ", th.fg(p.dim)));
        }
        let label = registry::category_label(c);
        cats.push(if k == 0 {
            Span::styled(label, th.strong(p.accent))
        } else {
            Span::styled(label, th.fg(p.fg))
        });
    }
    let how = match t.launch {
        Launch::Stream => "runs in a terminal",
        Launch::Gui => "desktop app",
        Launch::Web if t.run_cmd.trim().is_empty() => "opens in the browser",
        Launch::Web => "server + browser",
        Launch::Dataset => "wordlist data",
    };
    let enter = if g.icons { "⏎" } else { "Enter" };
    let state = match app.status[app.selected] {
        ToolStatus::Installed => vec![
            Span::styled(format!("{} ready", g.installed), th.fg(p.ok)),
            Span::styled(" · ", th.fg(p.dim)),
            Span::styled(enter, th.strong(p.accent)),
            Span::styled(format!(" {how}"), th.fg(p.dim)),
        ],
        // an invitation, in neutral colours, with the key to press
        ToolStatus::Missing => vec![
            Span::styled(
                format!("{} not installed · press ", g.missing),
                th.fg(p.dim),
            ),
            Span::styled("i", th.strong(p.accent)),
            Span::styled(" to install", th.fg(p.dim)),
        ],
        ToolStatus::Unknown => vec![Span::styled(
            format!("{} checking…", g.unknown),
            th.fg(p.dim),
        )],
        ToolStatus::Conflict => vec![Span::styled(
            format!("{} name conflict on PATH", g.conflict),
            th.fg(p.warn),
        )],
    };
    let lines = vec![
        Line::from(cats),
        Line::from(state),
        Line::from(Span::styled(t.description.clone(), th.fg(p.fg))),
    ];
    let block = th
        .block(p.border)
        .title(Span::styled(format!(" {} ", t.name), th.strong(p.fg)));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: true }),
        area,
    );
}

/// Browser-style tabs: `┌0 console┐ 1 nmap ⠋  2 ffuf ✓•  3 +  4 +`
fn render_terminal_tabs(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let g = th.glyph;
    let spin = g.spinner;
    let frame = spin[(app.tick / 2 % spin.len() as u64) as usize];
    let mut spans = vec![Span::raw(" ")];
    for pos in 0..=SLOTS {
        let sess = app.session(pos);
        let label = match sess {
            Some(s) => format!("{pos} {}", s.title),
            None => format!("{pos} +"),
        };
        if pos == app.focus {
            let mut on = th.strong(p.accent);
            if th.mono {
                on = on.add_modifier(Modifier::UNDERLINED);
            }
            spans.push(Span::styled(format!("{}{label}{}", g.tab_l, g.tab_r), on));
        } else {
            let style = if sess.is_some() {
                th.fg(p.fg)
            } else {
                th.fg(p.dim)
            };
            spans.push(Span::styled(format!(" {label} "), style));
        }
        if let Some(s) = sess {
            match s.state {
                RunState::Running => spans.push(Span::styled(frame, th.fg(p.warn))),
                RunState::Idle => {}
                _ if s.failed() => spans.push(Span::styled(g.failed, th.fg(p.err))),
                _ => spans.push(Span::styled(g.done, th.fg(p.ok))),
            }
            if s.unread {
                spans.push(Span::styled(g.unread, th.strong(p.info)));
            }
        }
        spans.push(Span::raw(" "));
    }
    if !app.grid && app.slots.iter().filter(|s| s.is_some()).count() > 1 {
        spans.push(Span::styled("g grid", th.fg(p.dim)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_terminals(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    if !app.grid {
        render_session(f, app, th, area, app.focus, false);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    for (r, row) in rows.iter().enumerate() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(*row);
        for (c, cell) in cols.iter().enumerate() {
            render_session(f, app, th, *cell, 1 + r * 2 + c, true);
        }
    }
}

/// One terminal (or an empty slot) at position `pos`.
fn render_session(f: &mut Frame, app: &App, th: &Theme, area: Rect, pos: usize, grid: bool) {
    let p = &th.pal;
    let focused = pos == app.focus;
    let Some(s) = app.session(pos) else {
        let block = th
            .block(if grid && focused { p.accent } else { p.border })
            .title(Span::styled(format!(" {pos} free "), th.fg(p.dim)));
        let inner = block.inner(area);
        f.render_widget(block, area);
        render_free_slot(f, th, inner, pos);
        return;
    };

    let mut title = vec![Span::styled(
        if pos == 0 {
            " console ".to_string()
        } else {
            format!(" {pos} {} ", s.title)
        },
        th.strong(p.accent),
    )];
    if !s.subtitle.is_empty() {
        title.push(Span::styled(format!("· {} ", s.subtitle), th.fg(p.dim)));
    }

    // Lines are hard-wrapped here (not by Paragraph) so we know exactly how
    // many rows the tail takes: with word-wrap, long lines used to push the
    // newest output below the pane.
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let (rows, max_scroll) = visible_rows(&s.output, inner_width, inner_height, s.scroll);
    s.view.set((inner_height, max_scroll, inner_width));
    let text: Vec<Line> = if s.output.is_empty() {
        vec![Line::from(Span::styled(
            "no output yet",
            th.fg(p.dim).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        rows.into_iter()
            .map(|(i, chunk)| {
                let mut style = line_style(th, &s.output[i]);
                if s.search_hit == Some(i) {
                    style = th.emph(style.bg(p.hit_bg));
                }
                Line::from(Span::styled(chunk, style))
            })
            .collect()
    };

    let mut bottom = state_line(app, th, s);
    if s.scroll > 0 {
        bottom.push_span(Span::styled(
            format!(" ↑ {} rows up · End to follow ", s.scroll.min(max_scroll)),
            th.fg(p.info),
        ));
    }
    let border = if s.running() {
        p.warn
    } else if grid && focused {
        p.accent
    } else {
        p.border
    };
    let mut block = th.block(border).title(title).title_bottom(bottom);
    if focused && pos > 0 && !grid {
        let hint = if s.running() {
            " s input · c stop · x close "
        } else {
            " x close "
        };
        block = block
            .title_bottom(Line::from(Span::styled(hint, th.fg(p.dim))).alignment(Alignment::Right));
    }
    f.render_widget(Paragraph::new(text).block(block), area);
}

/// Bottom-left of a terminal: what its process is doing.
fn state_line(app: &App, th: &Theme, s: &Session) -> Line<'static> {
    let p = &th.pal;
    match s.state {
        RunState::Running => {
            let spin = th.glyph.spinner;
            let frame = spin[(app.tick / 2 % spin.len() as u64) as usize];
            let pid = s.pid.map(|p| format!("pid {p} ")).unwrap_or_default();
            Line::from(vec![
                Span::styled(format!(" {frame} running "), th.strong(p.warn)),
                Span::styled(pid, th.fg(p.dim)),
            ])
        }
        RunState::Idle => Line::from(Span::styled(" idle ", th.fg(p.dim))),
        RunState::Killed => Line::from(Span::styled(" stopped ", th.fg(p.err))),
        RunState::Exited(code) => {
            let text = match code {
                Some(c) => format!(" exit {c} "),
                None => " ended ".to_string(),
            };
            let c = if s.failed() { p.err } else { p.ok };
            Line::from(Span::styled(text, th.fg(c)))
        }
    }
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
        Mode::SlotPick => {
            let mut spans = vec![
                Span::styled(
                    format!(" all {SLOTS} terminals busy {arrow} "),
                    pill(th, p.warn),
                ),
                Span::raw(" "),
            ];
            for pos in 1..=SLOTS {
                if let Some(s) = app.session(pos) {
                    spans.push(Span::styled(format!("{pos}"), th.strong(p.accent)));
                    spans.push(Span::styled(format!(" {}  ", s.title), th.fg(p.fg)));
                }
            }
            spans.push(Span::styled(
                "press a number to stop that tool and use its terminal · Esc cancel",
                th.fg(p.dim),
            ));
            Line::from(spans)
        }
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
                Span::styled(label, pill(th, p.warn)),
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
            // plain text: only a pending "press again" gets a solid label
            let status = if app.confirm.is_some() {
                Span::styled(format!(" {} ", app.status_line), pill(th, p.err))
            } else {
                Span::styled(format!(" {}", app.status_line), th.fg(p.fg))
            };
            Line::from(vec![
                status,
                Span::raw("  "),
                key("Enter"),
                sep(" run · "),
                key("i"),
                sep(" install · "),
                key("←→"),
                sep(" category · "),
                key("/"),
                sep(" filter · "),
                key("0-4"),
                sep(" terminals · "),
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

/// The inside of a free terminal: a little terminal drawing and what to
/// do next, or one line when the pane is small (grid cells).
fn render_free_slot(f: &mut Frame, th: &Theme, inner: Rect, pos: usize) {
    let p = &th.pal;
    let fancy = th.glyph.icons;
    let hint = if fancy {
        "⏎ runs the selected tool here"
    } else {
        "Enter runs the selected tool here"
    };
    let art: [&str; 3] = if fancy {
        ["╭──────────╮", "│ >_       │", "╰──────────╯"]
    } else {
        ["+----------+", "| >_       |", "+----------+"]
    };
    let mut lines: Vec<Line> = Vec::new();
    let big = inner.height >= 7 && inner.width >= 36;
    let body = if big { 6 } else { 1 };
    for _ in 0..inner.height.saturating_sub(body) / 2 {
        lines.push(Line::raw(""));
    }
    if big {
        for a in art {
            lines.push(Line::from(Span::styled(a, th.fg(p.border))));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("terminal {pos} is free"),
            th.strong(p.fg),
        )));
        lines.push(Line::from(Span::styled(hint, th.fg(p.dim))));
    } else {
        lines.push(Line::from(Span::styled(
            format!("{pos} · free · {hint}"),
            th.fg(p.dim),
        )));
    }
    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}

/// Corner notifications, newest at the bottom, bottom-right of the body.
fn render_toasts(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let p = &th.pal;
    let mut bottom = area.bottom();
    for t in app.toasts.iter().rev() {
        if bottom < area.y + 3 || area.width < 6 {
            break;
        }
        let text = format!(" {} ", t.text);
        let w = (Span::raw(text.as_str()).width() as u16 + 2).min(area.width);
        bottom -= 3;
        let r = Rect {
            x: area.right().saturating_sub(w + 1).max(area.x),
            y: bottom,
            width: w,
            height: 3,
        };
        let edge = match t.kind {
            ToastKind::Ok => p.ok,
            ToastKind::Err => p.err,
            ToastKind::Info => p.info,
        };
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, th.fg(p.fg)))).block(th.block(edge)),
            r,
        );
    }
}

/// What each step of the welcome tour says.
pub const TOUR: [&str; TOUR_STEPS] = [
    "↑ ↓ move through the tools · ← → switch category",
    "⏎ runs a tool in its own terminal · i installs it",
    "/ filters · ? shows every key · t changes the look",
];

/// The first-run welcome card, always centered, on top of everything.
fn render_tour(f: &mut Frame, th: &Theme, area: Rect, step: usize) {
    let p = &th.pal;
    let g = th.glyph;
    let step = step.min(TOUR_STEPS - 1);
    let text = TOUR[step];
    let mut dots = Vec::new();
    for i in 0..TOUR_STEPS {
        if i > 0 {
            dots.push(Span::raw(" "));
        }
        dots.push(if i == step {
            Span::styled(g.installed, th.strong(p.accent))
        } else {
            Span::styled(g.missing, th.fg(p.dim))
        });
    }
    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(text, th.strong(p.fg))),
        Line::raw(""),
        Line::from(dots),
    ];
    let hint = if step + 1 == TOUR_STEPS {
        "⏎ let's go · ← back · Esc skip"
    } else {
        "⏎ next · ← back · Esc skip"
    };
    let w = (Span::raw(text).width() as u16 + 8).max(54);
    dim_backdrop(f, th, area);
    let card = centered(area, w, 8);
    f.render_widget(Clear, card);
    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .block(overlay_block(th, "welcome to winsploit", hint)),
        card,
    );
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
        assert!(vis.iter().all(|&i| a.tools[i].in_category("reversing")));
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

    fn screen(t: &Terminal<TestBackend>, h: u16) -> String {
        (0..h).map(|y| row(t, y)).collect::<Vec<_>>().join("\n")
    }

    /// A terminal in slot `slot` with no real process behind it.
    fn put(a: &mut App, slot: usize, name: &str, out: &[&str], state: RunState) {
        let mut s = Session::new(100 + slot as u64, name, "desc");
        for l in out {
            s.push(*l);
        }
        s.state = state;
        a.slots[slot] = Some(s);
    }

    #[test]
    fn category_bar_list_and_card() {
        let mut a = app();
        let t = draw(&a, 140, 40);
        let sc = screen(&t, 40);
        assert!(sc.contains("All 45"), "{sc}");
        assert!(sc.contains("OSINT"), "{sc}");
        // the card names the selected tool's main category
        let first = &a.tools[a.selected];
        assert!(sc.contains(&registry::category_label(first.primary())));
        // the active tab scrolls into view even on a narrow screen
        a.cat = a.cat_tabs.len() - 1;
        let t = draw(&a, 80, 24);
        assert!(row(&t, 1).contains("Wordlists"), "{}", row(&t, 1));
        // an empty category under a filter says where the matches are
        a.cat = a
            .cat_tabs
            .iter()
            .position(|t| t.label() == "Stego")
            .unwrap();
        a.filter = "nmap".into();
        let t = draw(&a, 120, 30);
        assert!(screen(&t, 30).contains("in All"));
    }

    #[test]
    fn terminals_tabs_single_and_grid() {
        let mut a = app();
        put(
            &mut a,
            0,
            "nmap",
            &["22/tcp open"],
            RunState::Exited(Some(0)),
        );
        put(&mut a, 2, "ffuf", &["[err] boom"], RunState::Killed);
        a.slots[2].as_mut().unwrap().unread = true;
        a.focus = 1;
        let t = draw(&a, 140, 30);
        let sc = screen(&t, 30);
        for k in [
            "0 console",
            "1 nmap",
            "3 ffuf",
            "4 +",
            "22/tcp open",
            "exit 0",
            "x close",
        ] {
            assert!(sc.contains(k), "missing {k:?}:\n{sc}");
        }
        assert!(
            sc.contains('✓') && sc.contains('✗') && sc.contains('•'),
            "{sc}"
        );
        a.grid = true;
        let t = draw(&a, 140, 30);
        let sc = screen(&t, 30);
        for k in ["22/tcp open", "[err] boom", "2 free", "stopped"] {
            assert!(sc.contains(k), "grid missing {k:?}:\n{sc}");
        }
        for (w, h) in [(80, 24), (40, 10), (10, 4)] {
            draw(&a, w, h);
        }
    }

    #[test]
    fn slot_pick_footer_and_confirm() {
        let mut a = app();
        for i in 0..SLOTS {
            put(&mut a, i, &format!("t{i}"), &[], RunState::Running);
        }
        a.mode = Mode::SlotPick;
        let t = draw(&a, 160, 20);
        let footer = row(&t, 19);
        assert!(
            footer.contains("terminals busy") && footer.contains("Esc cancel"),
            "{footer}"
        );
        a.mode = Mode::Browsing;
        a.confirm = Some(crate::app::Confirm::Quit);
        draw(&a, 100, 20);
    }

    #[test]
    fn no_color_selection_is_reverse_video() {
        let mut a = app();
        a.mono = true;
        let t = draw(&a, 140, 40);
        let buf = t.backend().buffer();
        // row 3 = first list row (header, categories, list border above it);
        // x = 5 is inside the bar, past the glow on the left
        assert!(
            buf[(5, 3)].modifier.contains(Modifier::REVERSED),
            "{}",
            row(&t, 3)
        );
    }

    #[test]
    fn missing_tools_are_not_drawn_as_errors() {
        let mut a = app();
        a.status = vec![ToolStatus::Missing; a.tools.len()];
        let err = crate::theme::Palette::of(a.settings.theme).err;
        let t = draw(&a, 140, 40);
        let buf = t.backend().buffer();
        // the left pane: tool list and card
        for y in 2..39 {
            for x in 0..50 {
                assert_ne!(buf[(x, y)].fg, err, "red at ({x},{y}): {}", row(&t, y));
            }
        }
    }

    #[test]
    fn categories_get_their_own_row() {
        let mut a = app();
        let t = draw(&a, 140, 40);
        assert!(row(&t, 1).contains("All 45"), "{}", row(&t, 1));
        assert!(!row(&t, 1).contains("console"), "{}", row(&t, 1));
        assert!(row(&t, 2).contains("console"), "{}", row(&t, 2));
        // chips hidden on the right are flagged
        let t = draw(&a, 80, 24);
        assert!(row(&t, 1).trim_end().ends_with('›'), "{}", row(&t, 1));
        // and on the left once scrolled to the end
        a.cat = a.cat_tabs.len() - 1;
        let t = draw(&a, 80, 24);
        assert!(row(&t, 1).trim_start().starts_with('‹'), "{}", row(&t, 1));
    }

    #[test]
    fn toasts_and_tour_render_at_any_size() {
        let mut a = app();
        a.toast(ToastKind::Ok, "✓ nmap finished in terminal 1");
        a.toast(ToastKind::Err, "✗ ffuf exited 1");
        let t = draw(&a, 140, 40);
        let sc = screen(&t, 40);
        assert!(
            sc.contains("nmap finished") && sc.contains("ffuf exited"),
            "{sc}"
        );
        for step in 0..TOUR_STEPS {
            a.tour = Some(step);
            let t = draw(&a, 140, 40);
            assert!(screen(&t, 40).contains("welcome to winsploit"));
        }
        for (w, h) in [(80, 24), (40, 10), (10, 4)] {
            draw(&a, w, h);
        }
    }

    #[test]
    fn long_rows_keep_their_badge_inside_the_pane() {
        let mut a = app();
        a.status = vec![ToolStatus::Missing; a.tools.len()];
        let t = draw(&a, 140, 40);
        // the left pane is 50 wide: its right border is at x = 49
        for y in 4..30 {
            let r: String = row(&t, y).chars().take(50).collect();
            // the full badge is there, inside the border
            assert!(r.contains("+ get ") && r.ends_with('│'), "row {y}: {r}");
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
