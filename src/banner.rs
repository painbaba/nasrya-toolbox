//! ASCII-art banner and splash screen for winsploit.

use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;

/// "WINSPLOIT" in the ANSI-Shadow figlet style. Used by main.rs help text too.
pub const LOGO: &str = r#"
 ██╗    ██╗██╗███╗   ██╗███████╗██████╗ ██╗      ██████╗ ██╗████████╗
 ██║    ██║██║████╗  ██║██╔════╝██╔══██╗██║     ██╔═══██╗██║╚══██╔══╝
 ██║ █╗ ██║██║██╔██╗ ██║███████╗██████╔╝██║     ██║   ██║██║   ██║
 ██║███╗██║██║██║╚██╗██║╚════██║██╔═══╝ ██║     ██║   ██║██║   ██║
 ╚███╔███╔╝██║██║ ╚████║███████║██║     ███████╗╚██████╔╝██║   ██║
  ╚══╝╚══╝ ╚═╝╚═╝  ╚═══╝╚══════╝╚═╝     ╚══════╝ ╚═════╝ ╚═╝   ╚═╝
"#;

/// Linear interpolation between two bytes.
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

/// The theme's three-stop logo gradient, sampled at `t` in [0, 1].
fn gradient(stops: &[(u8, u8, u8); 3], t: f32) -> (u8, u8, u8) {
    const STOPS: usize = 3;
    let t = t.clamp(0.0, 1.0);
    let scaled = t * (STOPS as f32 - 1.0);
    let i = (scaled.floor() as usize).min(STOPS - 2);
    let local = scaled - i as f32;
    let (ar, ag, ab) = stops[i];
    let (br, bg, bb) = stops[i + 1];
    (
        lerp(ar, br, local),
        lerp(ag, bg, local),
        lerp(ab, bb, local),
    )
}

/// Mix a colour toward white by `amount` (0..=1) for a glowing highlight.
fn glow(c: (u8, u8, u8), amount: f32) -> (u8, u8, u8) {
    (
        lerp(c.0, 255, amount),
        lerp(c.1, 255, amount),
        lerp(c.2, 255, amount),
    )
}

/// Render the full-screen splash shown at startup.
pub fn render_splash(f: &mut Frame, tick: u64, th: &Theme) {
    let area = f.area();
    let p = &th.pal;

    // Gently pulsing border around the whole screen (solid on ANSI themes).
    let border_pulse = ((tick / 4) % 6) as f32 / 5.0;
    let border_color = match p.gradient {
        Some(stops) => {
            let bc = glow(
                gradient(&stops, 0.5),
                0.25 * (1.0 - (border_pulse - 0.5).abs() * 2.0).max(0.0),
            );
            Color::Rgb(bc.0, bc.1, bc.2)
        }
        None => p.accent,
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_set(th.border)
        .border_style(Style::default().fg(border_color));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let logo_lines: Vec<&str> = LOGO.lines().filter(|l| !l.is_empty()).collect();
    let logo_h = logo_lines.len() as u16;

    // Block layout: pad / logo / gap / subtitle / gap / prompt / pad.
    let total = logo_h + 1 + 1 + 1 + 1;
    let top_pad = inner.height.saturating_sub(total) / 2;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(logo_h),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    // A scanning glow sweeps down the logo, line by line.
    let n = logo_lines.len().max(1);
    let glow_idx = (tick / 2) % n as u64;
    let logo: Vec<Line> = logo_lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let t = if n > 1 {
                i as f32 / (n as f32 - 1.0)
            } else {
                0.0
            };
            let fg = match p.gradient {
                Some(stops) => {
                    let mut c = gradient(&stops, t);
                    if i as u64 == glow_idx {
                        c = glow(c, 0.5);
                    }
                    Color::Rgb(c.0, c.1, c.2)
                }
                None => p.accent,
            };
            Line::from(Span::styled(l.to_string(), th.strong(fg))).alignment(Alignment::Center)
        })
        .collect();
    f.render_widget(Paragraph::new(logo), rows[1]);

    // Subtitle.
    let subtitle = Line::from(vec![
        Span::styled("native-windows security toolbox", th.fg(p.fg)),
        Span::styled("  ·  ", th.fg(p.dim)),
        Span::styled("command runner", th.fg(p.fg)),
    ])
    .alignment(Alignment::Center);
    f.render_widget(Paragraph::new(subtitle), rows[3]);

    // Blinking "press any key" prompt.
    let on = (tick / 5).is_multiple_of(2);
    let prompt = if on {
        Line::from(vec![
            Span::styled(format!("{} ", th.glyph.right), th.fg(p.accent)),
            Span::styled("press any key to enter", th.strong(p.fg)),
            Span::styled(format!(" {}", th.glyph.left), th.fg(p.accent)),
        ])
    } else {
        Line::from(Span::styled("  press any key to enter  ", th.fg(p.dim)))
    }
    .alignment(Alignment::Center);
    f.render_widget(Paragraph::new(prompt), rows[5]);
}

/// A compact one-line wordmark for the main dashboard header.
pub fn wordmark(th: &Theme) -> Line<'static> {
    let p = &th.pal;
    Line::from(vec![
        // accent text, no pill: the solid bar is reserved for the selection
        Span::styled(
            if th.glyph.icons {
                " ⚡ winsploit"
            } else {
                " winsploit"
            },
            th.strong(p.accent),
        ),
        Span::styled("  ", Style::default()),
        Span::styled("native-windows security toolbox", th.fg(p.fg)),
        Span::styled("  ·  ", th.fg(p.dim)),
        Span::styled("command runner", th.fg(p.dim)),
    ])
}
