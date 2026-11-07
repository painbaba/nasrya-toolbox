//! Application state and the main event loop.
//!
//! Tools run in up to `SLOTS` independent terminals (plus the console, which
//! holds installs, hash ids and messages); the tool list is browsed by
//! category, and a tool can sit in several categories.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::args;
use crate::banner;
use crate::detect;
use crate::editor::{History, LineEditor};
use crate::envpath;
use crate::hashid;
use crate::install;
use crate::registry::{self, Catalog, Install, Launch, Tool};
use crate::runner::{self, AppEvent};
use crate::scoop;
use crate::session::{Session, CONSOLE, SLOTS};
use crate::theme::{Field, Settings};
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Unknown,
    Installed,
    Missing,
    /// the exe on PATH failed its `verify` check (a same-named program)
    Conflict,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Mode {
    Browsing,
    /// editing the selected tool's arguments
    Input,
    /// typing a hash for the built-in identifier
    HashInput,
    /// typing a line for the focused terminal's stdin
    StdinInput,
    /// typing a filter for the tool list (applied live)
    Filter,
    /// typing text to find in the output
    Search,
    /// every terminal is busy: pick one (1-4) to replace, or Esc
    SlotPick,
}

impl Mode {
    /// Modes that type into the line editor.
    pub fn is_prompt(&self) -> bool {
        !matches!(self, Mode::Browsing | Mode::SlotPick)
    }
}

/// One tab of the category bar.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CatTab {
    All,
    Installed,
    Cat(String),
}

impl CatTab {
    pub fn label(&self) -> String {
        match self {
            CatTab::All => "All".into(),
            CatTab::Installed => "Installed".into(),
            CatTab::Cat(id) => registry::category_label(id),
        }
    }
}

/// A second press is needed for these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Confirm {
    /// close terminal N (1-based) although its tool is running
    Close(usize),
    /// quit although tools are running
    Quit,
}

/// State of the `t` appearance overlay.
pub struct Appearance {
    /// highlighted row, an index into `theme::Field::ALL`
    pub cursor: usize,
    /// what to restore on Esc
    pub saved: Settings,
}

#[derive(PartialEq, Eq)]
enum Screen {
    Splash,
    Main,
}

pub struct App {
    pub tools: Vec<Tool>,
    buckets: BTreeMap<String, String>,
    pub status: Vec<ToolStatus>,
    /// For tools in Conflict: the path of the program that's actually found.
    conflicts: Vec<Option<PathBuf>>,
    pub selected: usize,
    /// Tool-list filter (empty = show everything in the category).
    pub filter: String,
    /// The category bar, and which tab is active.
    pub cat_tabs: Vec<CatTab>,
    pub cat: usize,
    /// Installs, hash ids and messages. Always there, can't be closed.
    pub console: Session,
    /// Tool terminals 1..=SLOTS.
    pub slots: [Option<Session>; SLOTS],
    /// Terminal in front: 0 = console, 1..=SLOTS = a slot.
    pub focus: usize,
    /// Show all slots in a 2x2 grid instead of one terminal.
    pub grid: bool,
    /// Last find-in-output text.
    search: String,
    pub mode: Mode,
    pub editor: LineEditor,
    history: History,
    /// A key that needs a second press (x on a running tool, q with tools running).
    pub confirm: Option<Confirm>,
    /// Run waiting for the user to pick a slot: (tool index, args).
    pending: Option<(usize, Vec<String>)>,
    next_id: u64,
    /// Counts finished processes, to find the oldest finished terminal.
    ended_seq: u64,
    pub status_line: String,
    pub tick: u64,
    /// The '?' key overlay is open.
    pub show_help: bool,
    /// Appearance in effect (live-previewed while the `t` overlay is open).
    pub settings: Settings,
    /// The `t` appearance overlay, when open.
    pub appearance: Option<Appearance>,
    /// A background detection pass is in flight.
    detecting: bool,
    /// The console is running a scoop install (so re-detect when it exits).
    installing: bool,
    screen: Screen,
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    quit: bool,
}

impl App {
    pub fn new(catalog: Catalog, catalog_path: PathBuf, ignored: Option<PathBuf>) -> Self {
        let Catalog { tools, buckets } = catalog;
        let (tx, rx) = mpsc::channel();
        let n = tools.len();
        let mut console = Session::new(CONSOLE, "console", "installs · hash ids · messages");
        console.push(":: winsploit ready. Pick a tool on the left; ← → switch category.");
        console.push(format!(
            ":: Enter runs it in its own terminal (up to {SLOTS} at once) · i installs it."
        ));
        console.push(format!(":: catalog: {}", catalog_path.display()));
        if let Some(ignored) = ignored {
            console.push(format!(
                "[err] ignored {} — catalogs aren't loaded from the current directory \
                 unless you pass --catalog",
                ignored.display()
            ));
        }
        let cat_tabs = Self::build_tabs(&tools);
        let mut app = App {
            tools,
            buckets,
            status: vec![ToolStatus::Unknown; n],
            conflicts: vec![None; n],
            selected: 0,
            filter: String::new(),
            cat_tabs,
            cat: 0,
            console,
            slots: Default::default(),
            focus: 0,
            grid: false,
            search: String::new(),
            mode: Mode::Browsing,
            editor: LineEditor::default(),
            history: History::default(),
            confirm: None,
            pending: None,
            next_id: CONSOLE,
            ended_seq: 0,
            status_line: "detecting tools…".into(),
            tick: 0,
            show_help: false,
            settings: Settings::default(),
            appearance: None,
            detecting: false,
            installing: false,
            screen: Screen::Splash,
            tx,
            rx,
            quit: false,
        };
        // start on the first tool in list order, not catalog order
        if let Some(&first) = app.visible().first() {
            app.selected = first;
        }
        app
    }

    /// All, Installed, then every category in use: known ones in taxonomy
    /// order, unknown ones alphabetically after.
    fn build_tabs(tools: &[Tool]) -> Vec<CatTab> {
        let mut ids: Vec<String> = Vec::new();
        for t in tools {
            let cats: Vec<String> = if t.categories.is_empty() {
                vec!["other".into()]
            } else {
                t.categories.clone()
            };
            for c in cats {
                if !ids.contains(&c) {
                    ids.push(c);
                }
            }
        }
        ids.sort_by(|a, b| {
            (registry::category_rank(a), a.as_str()).cmp(&(registry::category_rank(b), b.as_str()))
        });
        let mut tabs = vec![CatTab::All, CatTab::Installed];
        tabs.extend(ids.into_iter().map(CatTab::Cat));
        tabs
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.detect_all(false);
        while !self.quit {
            terminal.draw(|f| match self.screen {
                Screen::Splash => banner::render_splash(f, self.tick, &self.settings.theme()),
                Screen::Main => ui::render(f, &self),
            })?;

            // drain runner events
            while let Ok(ev) = self.rx.try_recv() {
                self.on_runner_event(ev);
            }

            if event::poll(Duration::from_millis(80))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => match self.screen {
                        Screen::Splash => self.screen = Screen::Main,
                        Screen::Main => self.on_key(key),
                    },
                    // bracketed paste: the whole pasted text arrives at once
                    Event::Paste(text) if self.mode.is_prompt() => self.editor.insert_str(&text),
                    _ => {}
                }
            }
            self.tick = self.tick.wrapping_add(1);
        }
        self.kill_all();
        Ok(())
    }

    // ------------------------------------------------------------ terminals

    /// The terminal at position `pos` (0 = console, 1..=SLOTS = slots).
    pub fn session(&self, pos: usize) -> Option<&Session> {
        if pos == 0 {
            Some(&self.console)
        } else {
            self.slots.get(pos - 1).and_then(Option::as_ref)
        }
    }

    fn session_mut(&mut self, pos: usize) -> Option<&mut Session> {
        if pos == 0 {
            Some(&mut self.console)
        } else {
            self.slots.get_mut(pos - 1).and_then(Option::as_mut)
        }
    }

    pub fn focused(&self) -> Option<&Session> {
        self.session(self.focus)
    }

    fn focused_mut(&mut self) -> Option<&mut Session> {
        self.session_mut(self.focus)
    }

    /// Where the session with this id lives, if it still exists.
    fn position_of(&self, sid: u64) -> Option<usize> {
        if sid == CONSOLE {
            return Some(0);
        }
        self.slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.id == sid))
            .map(|i| i + 1)
    }

    /// Is the terminal at `pos` on screen right now?
    fn on_screen(&self, pos: usize) -> bool {
        pos == self.focus || (self.grid && pos > 0)
    }

    fn set_focus(&mut self, pos: usize) {
        self.focus = pos.min(SLOTS);
        if self.focus == 0 {
            self.grid = false;
        }
        if let Some(s) = self.focused_mut() {
            s.unread = false;
        }
    }

    /// Tab / Shift+Tab: the next terminal that has something in it (the
    /// console counts, except in the grid, which only shows slots).
    fn cycle_focus(&mut self, forward: bool) {
        let n = SLOTS + 1;
        let mut pos = self.focus;
        for _ in 0..n {
            pos = if forward {
                (pos + 1) % n
            } else {
                (pos + n - 1) % n
            };
            let usable = if pos == 0 {
                !self.grid
            } else {
                self.slots[pos - 1].is_some()
            };
            if usable {
                self.set_focus(pos);
                return;
            }
        }
    }

    pub fn running_count(&self) -> usize {
        self.slots.iter().flatten().filter(|s| s.running()).count()
            + usize::from(self.console.running())
    }

    /// The slot a new run goes to: the first empty one, else the one whose
    /// tool finished longest ago. None when every slot is still running.
    pub fn free_slot(&self) -> Option<usize> {
        if let Some(i) = self.slots.iter().position(Option::is_none) {
            return Some(i);
        }
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().filter(|s| !s.running()).map(|s| (s.ended, i)))
            .min()
            .map(|(_, i)| i)
    }

    /// Start `tool` with `args` in slot `slot` (0-based), replacing (and
    /// killing) whatever is there, and bring it to the front.
    fn start_tool(&mut self, slot: usize, tool: usize, args: Vec<String>) {
        if let Some(mut old) = self.slots[slot].take() {
            if old.running() {
                old.kill();
                old.exited(None, 0); // closes its log
            }
        }
        let t = self.tools[tool].clone();
        self.next_id += 1;
        let mut s = Session::new(self.next_id, &t.name, &t.description);
        if let Some(note) = &t.note {
            s.push(format!(":: note: {note}"));
        }
        let shown = format!("{} {}", t.run_cmd, args::join(&args));
        s.push(format!("$ {shown}"));
        let cmd = runner::tool_command(&t.run_cmd, &args);
        match runner::spawn_streaming(cmd, self.tx.clone(), true, s.id) {
            Ok(sp) => {
                s.open_log(&t.name, &shown);
                self.status_line = format!(
                    "running {} in terminal {} (pid {})",
                    t.name,
                    slot + 1,
                    sp.pid
                );
                s.started(sp.pid, sp.stdin);
                if t.launch == Launch::Web {
                    // a local web app: give the server a moment, then open it
                    let url = t.url.clone().unwrap_or_default();
                    s.push(format!(":: opening {url} in your browser"));
                    self.open_url_later(s.id, url, 2);
                }
            }
            Err(e) => s.push(format!("[err] failed to launch: {e}")),
        }
        self.slots[slot] = Some(s);
        self.set_focus(slot + 1);
    }

    /// Run in a free slot, or ask which busy one to replace.
    fn run_in_slot(&mut self, tool: usize, args: Vec<String>) {
        match self.free_slot() {
            Some(slot) => self.start_tool(slot, tool, args),
            None => {
                self.pending = Some((tool, args));
                self.mode = Mode::SlotPick;
            }
        }
    }

    /// `x`: close the focused terminal. A running tool needs a second `x`.
    fn close_focused(&mut self, confirmed: bool) {
        if self.focus == 0 {
            self.status_line = "the console stays open (it holds installs and messages)".into();
            return;
        }
        let pos = self.focus;
        let Some(s) = self.slots[pos - 1].as_ref() else {
            self.status_line = format!("terminal {pos} is already free");
            return;
        };
        if s.running() && !confirmed {
            self.status_line = format!(
                "{} is still running — press x again to stop it and close terminal {pos}",
                s.title
            );
            self.confirm = Some(Confirm::Close(pos));
            return;
        }
        let mut s = self.slots[pos - 1].take().expect("checked above");
        if s.running() {
            s.kill();
            s.exited(None, 0);
        }
        self.status_line = format!("closed terminal {pos} ({}) · its log is kept", s.title);
        if !self.grid {
            // show a neighbour that still has something in it, else the console
            let next = (1..=SLOTS)
                .filter(|&p| self.slots[p - 1].is_some())
                .min_by_key(|&p| p.abs_diff(pos))
                .unwrap_or(0);
            self.set_focus(next);
        }
    }

    /// Stop every running process (on quit).
    fn kill_all(&mut self) {
        for s in self.slots.iter_mut().flatten() {
            s.kill();
        }
        self.console.kill();
    }

    // ----------------------------------------------------------- detection

    /// Re-probe every tool on a background thread; the result comes back as
    /// `AppEvent::Detected`, so the UI keeps drawing meanwhile. With
    /// `refresh_path`, first pick up PATH changes persisted since winsploit
    /// started (installs that add a PATH entry, a freshly installed Scoop).
    fn detect_all(&mut self, refresh_path: bool) {
        if self.detecting {
            return;
        }
        self.detecting = true;
        self.status_line = "detecting tools…".into();
        let probes: Vec<(String, Option<detect::Verify>)> = self
            .tools
            .iter()
            .map(|t| (t.exe.clone(), t.verify.clone()))
            .collect();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if refresh_path {
                for dir in envpath::refresh() {
                    let line = format!(":: PATH: picked up {dir}");
                    let _ = tx.send(AppEvent::Output { sid: CONSOLE, line });
                }
            }
            let found = detect::probe_all(&probes);
            let _ = tx.send(AppEvent::Detected(found));
        });
    }

    fn on_detected(&mut self, found: Vec<detect::Found>) {
        self.detecting = false;
        self.conflicts = vec![None; found.len()];
        self.status = found
            .into_iter()
            .enumerate()
            .map(|(i, f)| match f {
                detect::Found::Installed(_) => ToolStatus::Installed,
                detect::Found::Missing => ToolStatus::Missing,
                detect::Found::Conflict(p) => {
                    self.conflicts[i] = Some(p);
                    ToolStatus::Conflict
                }
            })
            .collect();
        let count = |s: ToolStatus| self.status.iter().filter(|x| **x == s).count();
        let installed = count(ToolStatus::Installed);
        let conflicts = count(ToolStatus::Conflict);
        self.status_line = format!("{installed}/{} tools installed", self.tools.len());
        if conflicts > 0 {
            self.status_line
                .push_str(&format!(" · {conflicts} name conflict(s)"));
        }
        // the Installed tab may have changed under the selection
        self.keep_selection_visible();
    }

    fn on_runner_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Output { sid, line } => {
                let Some(pos) = self.position_of(sid) else {
                    return; // its terminal was closed or replaced
                };
                let seen = self.on_screen(pos);
                if let Some(s) = self.session_mut(pos) {
                    s.output_line(line);
                    s.unread |= !seen;
                }
            }
            AppEvent::Exited { sid, code } => {
                self.ended_seq += 1;
                let seq = self.ended_seq;
                if let Some(pos) = self.position_of(sid) {
                    let seen = self.on_screen(pos);
                    let focused = pos == self.focus;
                    if let Some(s) = self.session_mut(pos) {
                        s.exited(code, seq);
                        s.unread |= !seen;
                        let title = s.title.clone();
                        if pos > 0 && !focused {
                            let how = match code {
                                Some(c) => format!("exit {c}"),
                                None => "ended".into(),
                            };
                            self.status_line =
                                format!("{title} finished in terminal {pos} ({how})");
                        }
                    }
                    if focused && self.mode == Mode::StdinInput {
                        self.mode = Mode::Browsing;
                        self.editor.set("");
                    }
                }
                // Only an install can change what's on PATH; a plain tool run
                // doesn't need a re-probe.
                if sid == CONSOLE && std::mem::take(&mut self.installing) {
                    self.detect_all(true);
                }
            }
            AppEvent::Detected(found) => self.on_detected(found),
        }
    }

    // ----------------------------------------------------------------- keys

    fn on_key(&mut self, key: KeyEvent) {
        if self.appearance.is_some() {
            self.on_key_appearance(key.code);
            return;
        }
        if self.show_help {
            // any key closes the overlay
            self.show_help = false;
            return;
        }
        match self.mode {
            Mode::Browsing => self.on_key_browsing(key),
            Mode::SlotPick => self.on_key_slot_pick(key.code),
            _ => self.on_key_input(key),
        }
    }

    /// Keys while the appearance overlay is open. Changes apply live;
    /// Enter keeps and saves them, Esc puts the previous look back.
    fn on_key_appearance(&mut self, code: KeyCode) {
        let Some(ap) = self.appearance.as_mut() else {
            return;
        };
        let n = Field::ALL.len();
        let field = Field::ALL[ap.cursor];
        match code {
            KeyCode::Up | KeyCode::Char('k') => ap.cursor = (ap.cursor + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => ap.cursor = (ap.cursor + 1) % n,
            KeyCode::Left | KeyCode::Char('h') => self.settings.step(field, -1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                self.settings.step(field, 1)
            }
            // 1-7 pick a theme directly, as in Claude Code's /theme
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if let Some(t) = crate::theme::ThemeName::ALL.get(i) {
                    self.settings.theme = *t;
                    ap.cursor = 0;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.settings = ap.saved;
                self.appearance = None;
            }
            KeyCode::Enter => {
                self.appearance = None;
                match self.settings.save() {
                    Ok(path) => self.status_line = format!("appearance saved · {}", path.display()),
                    Err(e) => {
                        self.status_line = "appearance applied (not saved)".into();
                        self.console
                            .push(format!("[err] could not save appearance: {e}"));
                    }
                }
            }
            _ => {}
        }
    }

    /// All terminals busy: 1-4 replaces that one, Esc gives up.
    fn on_key_slot_pick(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(c @ '1'..='9') if (c as usize - '0' as usize) <= SLOTS => {
                self.mode = Mode::Browsing;
                if let Some((tool, args)) = self.pending.take() {
                    self.start_tool(c as usize - '1' as usize, tool, args);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Browsing;
                self.pending = None;
                self.status_line = "nothing was started".into();
            }
            _ => {}
        }
    }

    /// History key for the open prompt: the tool's name, or the hash prompt.
    fn history_key(&self) -> String {
        if self.mode == Mode::HashInput {
            "\u{0}hashid".to_string()
        } else if self.mode == Mode::StdinInput {
            "\u{0}stdin".to_string()
        } else if self.mode == Mode::Search {
            "\u{0}search".to_string()
        } else {
            self.tools[self.selected].name.clone()
        }
    }

    fn on_key_browsing(&mut self, key: KeyEvent) {
        // a pending confirmation only survives until the next key
        let confirm = self.confirm.take();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                let n = self.running_count();
                if n > 0 && confirm != Some(Confirm::Quit) {
                    self.status_line =
                        format!("{n} tool(s) still running — press q again to stop them and quit");
                    self.confirm = Some(Confirm::Quit);
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Left | KeyCode::Char('[') => self.step_category(-1),
            KeyCode::Right | KeyCode::Char(']') => self.step_category(1),
            KeyCode::Char(c @ '0'..='9') if (c as usize - '0' as usize) <= SLOTS => {
                self.set_focus(c as usize - '0' as usize)
            }
            KeyCode::Tab => self.cycle_focus(true),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Char('g') => {
                self.grid = !self.grid;
                if self.grid && self.focus == 0 {
                    let first = (1..=SLOTS)
                        .find(|&p| self.slots[p - 1].is_some())
                        .unwrap_or(1);
                    self.set_focus(first);
                }
            }
            KeyCode::Char('x') => self.close_focused(confirm == Some(Confirm::Close(self.focus))),
            KeyCode::Char('c') => self.cancel(),
            KeyCode::Char('s') => self.open_stdin(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('t') => {
                self.appearance = Some(Appearance {
                    cursor: 0,
                    saved: self.settings,
                })
            }
            KeyCode::Char('f') => {
                self.editor.set(&self.search.clone());
                self.mode = Mode::Search;
            }
            KeyCode::Char('n') => self.find_next(true),
            KeyCode::Char('N') => self.find_next(false),
            KeyCode::Char('/') => {
                self.editor.set(&self.filter.clone());
                self.mode = Mode::Filter;
            }
            KeyCode::Char('r') => self.detect_all(true),
            KeyCode::PageUp => self.focused_mut().map_or((), Session::page_up),
            KeyCode::PageDown => self.focused_mut().map_or((), Session::page_down),
            KeyCode::End => self.focused_mut().map_or((), Session::follow),
            KeyCode::Home => self.focused_mut().map_or((), Session::scroll_top),
            KeyCode::Char('h') => {
                self.editor.set("");
                self.history.reset();
                self.mode = Mode::HashInput;
            }
            KeyCode::Enter | KeyCode::Char('i') if !self.visible().contains(&self.selected) => {
                self.status_line = "no tool selected here (← → category, / filter)".into();
            }
            KeyCode::Char('i') => self.install_selected(),
            KeyCode::Enter => {
                if self.tools[self.selected].launch == Launch::Dataset {
                    // nothing to run, so no args prompt
                    self.show_dataset();
                } else {
                    let hint = self.tools[self.selected].args_hint.clone();
                    self.editor.set(&hint.unwrap_or_default());
                    self.history.reset();
                    self.mode = Mode::Input;
                }
            }
            _ => {}
        }
    }

    /// `s`: type into the focused terminal's tool.
    fn open_stdin(&mut self) {
        let state = self
            .focused()
            .map(|s| (s.running(), s.has_stdin(), s.title.clone()));
        match state {
            Some((true, true, _)) => {
                self.editor.set("");
                self.history.reset();
                self.mode = Mode::StdinInput;
            }
            Some((true, false, title)) => {
                self.status_line = format!("{title} isn't reading input (closed with Ctrl+D?)");
            }
            _ => self.status_line = "nothing is running in this terminal".into(),
        }
    }

    fn on_key_input(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let e = &mut self.editor;
        match key.code {
            KeyCode::Esc if self.mode == Mode::Filter => {
                // Esc in the filter prompt clears the filter
                self.editor.set("");
                self.set_filter(String::new());
                self.mode = Mode::Browsing;
            }
            KeyCode::Esc => {
                self.mode = Mode::Browsing;
                self.editor.set("");
                self.history.reset();
            }
            KeyCode::Enter if self.mode == Mode::Filter => {
                // keep the filter, back to the list
                self.editor.set("");
                self.mode = Mode::Browsing;
            }
            KeyCode::Up if self.mode == Mode::Filter => self.move_selection(-1),
            KeyCode::Down if self.mode == Mode::Filter => self.move_selection(1),
            KeyCode::Enter => {
                let text = self.editor.take();
                let hkey = self.history_key();
                self.history.push(&hkey, &text);
                let mode = std::mem::replace(&mut self.mode, Mode::Browsing);
                match mode {
                    Mode::Search => {
                        self.search = text;
                        if let Some(s) = self.focused_mut() {
                            s.search_hit = None;
                        }
                        self.find_next(true);
                    }
                    Mode::HashInput => self.identify_hash(&text),
                    Mode::StdinInput => {
                        if let Some(s) = self.focused_mut() {
                            s.send(&text)
                        }
                    }
                    _ => self.launch(&text),
                }
            }
            KeyCode::Up => {
                let hkey = self.history_key();
                let cur = self.editor.text();
                if let Some(line) = self.history.older(&hkey, &cur) {
                    self.editor.set(&line);
                }
            }
            KeyCode::Down => {
                let hkey = self.history_key();
                if let Some(line) = self.history.newer(&hkey) {
                    self.editor.set(&line);
                }
            }
            KeyCode::Left if ctrl => e.word_left(),
            KeyCode::Right if ctrl => e.word_right(),
            KeyCode::Left => e.left(),
            KeyCode::Right => e.right(),
            KeyCode::Home => e.home(),
            KeyCode::End => e.end(),
            KeyCode::Backspace if ctrl => e.delete_word_left(),
            KeyCode::Backspace => e.backspace(),
            KeyCode::Delete => e.delete(),
            KeyCode::Char('d') if ctrl && self.mode == Mode::StdinInput => {
                // end-of-input: close the pipe so the tool sees EOF
                self.mode = Mode::Browsing;
                self.editor.set("");
                if let Some(s) = self.focused_mut() {
                    s.close_stdin();
                }
            }
            KeyCode::Char('u') if ctrl => e.clear_to_start(),
            KeyCode::Char('w') if ctrl => e.delete_word_left(),
            KeyCode::Char('a') if ctrl => e.home(),
            KeyCode::Char('e') if ctrl => e.end(),
            // other Ctrl/Alt chords aren't text
            KeyCode::Char(_) if ctrl || alt => {}
            KeyCode::Char(c) => e.insert(c),
            _ => {}
        }
        if self.mode == Mode::Filter {
            let f = self.editor.text();
            if f != self.filter {
                self.set_filter(f);
            }
        }
    }

    /// Jump to the next match of the last search in the focused terminal:
    /// `older` = further up (n), otherwise further down (N).
    fn find_next(&mut self, older: bool) {
        let needle = self.search.trim().to_string();
        if needle.is_empty() {
            self.status_line = "nothing to find (press f)".into();
            return;
        }
        let Some(s) = self.focused_mut() else {
            self.status_line = "this terminal is empty".into();
            return;
        };
        self.status_line = match s.find_next(&needle, older) {
            Some(total) => format!("'{needle}': {total} matching line(s) · n older · N newer"),
            None => format!("'{needle}' not found in this terminal"),
        };
    }

    // ----------------------------------------------------------- tool list

    /// Is tool `i` in category tab `tab`?
    fn in_tab(&self, i: usize, tab: &CatTab) -> bool {
        match tab {
            CatTab::All => true,
            CatTab::Installed => self.status[i] == ToolStatus::Installed,
            CatTab::Cat(id) => self.tools[i].in_category(id),
        }
    }

    /// Does tool `i` match the filter? Words AND together, each matching
    /// the name, the description, or any of its categories.
    fn matches_filter(&self, i: usize) -> bool {
        let needle = self.filter.trim().to_lowercase();
        let t = &self.tools[i];
        needle.split_whitespace().all(|w| {
            t.name.to_lowercase().contains(w)
                || t.description.to_lowercase().contains(w)
                || t.categories.iter().any(|c| {
                    c.contains(w) || registry::category_label(c).to_lowercase().contains(w)
                })
        })
    }

    /// Tools in the active tab that match the filter, in display order.
    pub fn visible(&self) -> Vec<usize> {
        self.visible_in(&self.cat_tabs[self.cat])
    }

    /// Display order: in a category, the tools made for it come first, then
    /// the ones that are also useful there. Elsewhere, grouped by main
    /// category. Catalog order breaks ties.
    pub fn visible_in(&self, tab: &CatTab) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.tools.len())
            .filter(|&i| self.in_tab(i, tab) && self.matches_filter(i))
            .collect();
        match tab {
            CatTab::Cat(id) => v.sort_by_key(|&i| (self.tools[i].primary() != id, i)),
            _ => v.sort_by_key(|&i| {
                let p = self.tools[i].primary();
                (registry::category_rank(p), p.to_string(), i)
            }),
        }
        v
    }

    /// ← →: switch category tab.
    fn step_category(&mut self, delta: i32) {
        let n = self.cat_tabs.len() as i32;
        self.cat = (self.cat as i32 + delta).rem_euclid(n) as usize;
        self.keep_selection_visible();
        let tab = &self.cat_tabs[self.cat];
        self.status_line = match tab {
            CatTab::All => "all tools, grouped by main category".into(),
            CatTab::Installed => "tools installed on this machine".into(),
            CatTab::Cat(id) => {
                let blurb = registry::CATEGORIES
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.blurb)
                    .unwrap_or("");
                format!("{}: {blurb}", tab.label())
            }
        };
    }

    /// If the selected tool left the list, select the first one shown.
    fn keep_selection_visible(&mut self) {
        let vis = self.visible();
        if !vis.is_empty() && !vis.contains(&self.selected) {
            self.selected = vis[0];
        }
    }

    fn set_filter(&mut self, f: String) {
        self.filter = f;
        self.keep_selection_visible();
    }

    /// Move the selection by `delta` among the visible tools.
    fn move_selection(&mut self, delta: i32) {
        let vis = self.visible();
        let Some(pos) = vis.iter().position(|&i| i == self.selected) else {
            if let Some(&first) = vis.first() {
                self.selected = first;
            }
            return;
        };
        let new = (pos as i32 + delta).clamp(0, vis.len() as i32 - 1) as usize;
        self.selected = vis[new];
    }

    // --------------------------------------------------------------- actions

    /// Built-in hash identifier (`h`): no install, works offline.
    fn identify_hash(&mut self, hash: &str) {
        if hash.trim().is_empty() {
            return;
        }
        self.console.push(format!("$ hashid {}", hash.trim()));
        for line in hashid::report(hash) {
            self.console.push(line);
        }
        self.set_focus(0);
    }

    fn launch(&mut self, args_str: &str) {
        let status = self.status[self.selected];
        let tool = self.tools[self.selected].clone();
        let name = tool.name.clone();

        match status {
            ToolStatus::Installed => {}
            ToolStatus::Unknown => {
                self.status_line = "still detecting tools — try again in a moment".into();
                return;
            }
            ToolStatus::Missing => {
                self.status_line = format!("{name} is not installed — press i to install it");
                return;
            }
            ToolStatus::Conflict => {
                let path = self.conflicts[self.selected]
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.console.push(format!(
                    "[err] the '{}' on PATH ({path}) isn't {name} — a different program with \
                     the same name comes first. Not running it.",
                    tool.exe
                ));
                self.console.push(
                    "[err] fix: press i to install the right one, or move its folder earlier in \
                     PATH (or uninstall the other program), then press r.",
                );
                self.status_line = format!("{name}: name conflict, not run (see console)");
                self.set_focus(0);
                return;
            }
        }

        let args = match args::split(args_str) {
            Ok(a) => a,
            Err(e) => {
                self.status_line = format!("can't parse the args: {e} — nothing was run");
                return;
            }
        };

        match tool.launch {
            Launch::Stream => self.run_in_slot(self.selected, args),
            Launch::Web if !tool.run_cmd.trim().is_empty() => {
                // the server streams in a terminal; the browser opens after
                self.run_in_slot(self.selected, args)
            }
            Launch::Web => {
                let url = tool.url.clone().unwrap_or_default();
                self.console
                    .push(format!(":: {name}: opening {url} in your browser"));
                self.open_url_later(CONSOLE, url, 0);
                self.status_line = format!("opened {name} in your browser");
            }
            Launch::Gui => {
                self.console
                    .push(format!("$ {} {}", tool.run_cmd, args::join(&args)));
                match runner::spawn_detached(runner::tool_command(&tool.run_cmd, &args)) {
                    Ok(pid) => {
                        self.console.push(format!(
                            "── launched {name} (pid {pid}); it runs in its own window ──"
                        ));
                        self.status_line = format!("launched {name} in its own window");
                    }
                    Err(e) => {
                        self.console
                            .push(format!("[err] failed to launch {name}: {e}"));
                        self.status_line = format!("{name} failed to launch (see console)");
                    }
                }
            }
            Launch::Dataset => self.show_dataset(),
        }
    }

    /// Enter on a dataset: list its files and copy the folder path, so it can
    /// be pasted into another tool's arguments.
    fn show_dataset(&mut self) {
        let tool = self.tools[self.selected].clone();
        match self.status[self.selected] {
            ToolStatus::Installed => {}
            ToolStatus::Unknown => {
                self.status_line = "still detecting tools — try again in a moment".into();
                return;
            }
            _ => {
                self.status_line =
                    format!("{} is not installed — press i to install it", tool.name);
                return;
            }
        }
        let Some(dir) = std::path::Path::new(&tool.exe)
            .parent()
            .map(|d| d.to_path_buf())
        else {
            return;
        };
        let c = &mut self.console;
        c.push(format!("$ {} — {}", tool.name, dir.display()));
        let mut files: Vec<(String, u64)> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .map(|e| {
                        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                        (e.file_name().to_string_lossy().to_string(), size)
                    })
                    .filter(|(n, _)| {
                        !n.eq_ignore_ascii_case("manifest.json")
                            && !n.eq_ignore_ascii_case("install.json")
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        let w = files
            .iter()
            .map(|(n, _)| n.chars().count())
            .max()
            .unwrap_or(0);
        for (n, size) in files {
            c.push(format!("   {n:<w$}  {:>8.1} KB", size as f64 / 1024.0));
        }
        match runner::copy_to_clipboard(&dir.to_string_lossy()) {
            Ok(()) => c.push(":: folder path copied to the clipboard"),
            Err(e) => c.push(format!("[err] couldn't copy the path: {e}")),
        }
        self.set_focus(0);
    }

    /// Open `url` in the default browser after `delay_s` seconds; a failure
    /// is reported in terminal `sid`.
    fn open_url_later(&self, sid: u64, url: String, delay_s: u64) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(delay_s));
            if let Err(e) = runner::open_url(&url) {
                let line = format!("[err] couldn't open {url}: {e}");
                let _ = tx.send(AppEvent::Output { sid, line });
            }
        });
    }

    /// Is there a real `python`? The Microsoft Store puts a stub
    /// `python.exe` on PATH that only prints "Python was not found" (exit
    /// 9009), so resolving the name isn't enough; ask it for its version.
    fn python_works() -> bool {
        detect::resolve("python").is_some()
            && std::process::Command::new("python")
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    }

    /// Installs run in the console, one at a time, so they never take a
    /// tool terminal.
    fn install_selected(&mut self) {
        if self.console.running() {
            self.status_line = "an install is already running (see console, 0)".into();
            return;
        }
        let tool = self.tools[self.selected].clone();
        let name = tool.name.clone();

        let pipx = detect::resolve("pipx").is_some();
        let pre = install::Prereqs {
            scoop: detect::resolve("scoop").is_some(),
            pipx,
            // only worth the subprocess when pipx has to be bootstrapped
            python: tool.install == Install::Pipx && !pipx && Self::python_works(),
            bucket_added: tool.bucket.as_deref().is_some_and(scoop::bucket_added),
        };
        self.set_focus(0);
        let steps = match install::plan(&tool, pre, &self.buckets) {
            Ok(s) => s,
            Err(e) => {
                self.console
                    .push(format!("[err] can't install {name}: {e}"));
                return;
            }
        };
        let line = steps.join(" && ");
        self.console.push(format!("$ {line}"));

        // scoop and pipx are .cmd/.bat shims, so go through cmd. Every token
        // in `line` was checked by install::plan to be free of cmd
        // metacharacters, and `&&` stops at the first failing step. /D skips
        // any AutoRun commands from the registry.
        let mut cmd = std::process::Command::new("cmd");
        {
            use std::os::windows::process::CommandExt;
            cmd.raw_arg(format!("/D /C {line}"));
        }
        match runner::spawn_streaming(cmd, self.tx.clone(), false, CONSOLE) {
            Ok(sp) => {
                self.console.open_log(&format!("install-{name}"), &line);
                self.console.started(sp.pid, None);
                self.installing = true;
                self.status_line = format!("installing {name}…");
            }
            Err(e) => self
                .console
                .push(format!("[err] failed to start the installer: {e}")),
        }
    }

    /// `c`: stop the focused terminal's tool; its output stays.
    fn cancel(&mut self) {
        let pos = self.focus;
        match self.focused_mut().and_then(Session::kill) {
            Some(pid) => {
                self.status_line = format!("stopping pid {pid} · x closes the terminal");
            }
            None if pos == 0 => self.status_line = "no install is running".into(),
            None => self.status_line = "nothing is running in this terminal".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::RunState;

    fn app() -> App {
        let cat = crate::registry::parse(include_str!("../tools.yaml")).unwrap();
        App::new(cat, PathBuf::from("tools.yaml"), None)
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    /// A terminal holding `name`, running or finished at `ended`. No real
    /// process (pid stays None), so kill() never calls taskkill.
    fn fake(app: &mut App, slot: usize, name: &str, running: bool, ended: u64) {
        app.next_id += 1;
        let mut s = Session::new(app.next_id, name, "");
        s.state = if running {
            RunState::Running
        } else {
            RunState::Exited(Some(0))
        };
        s.ended = ended;
        app.slots[slot] = Some(s);
    }

    fn tab(a: &App, label: &str) -> usize {
        a.cat_tabs.iter().position(|t| t.label() == label).unwrap()
    }

    fn names(a: &App) -> Vec<String> {
        a.visible()
            .iter()
            .map(|&i| a.tools[i].name.clone())
            .collect()
    }

    #[test]
    fn category_tabs_follow_the_taxonomy() {
        let a = app();
        let labels: Vec<String> = a.cat_tabs.iter().map(CatTab::label).collect();
        assert_eq!(&labels[..4], ["All", "Installed", "OSINT", "Recon"]);
        assert_eq!(labels.last().unwrap(), "Wordlists");
    }

    #[test]
    fn a_tool_shows_in_every_category_it_has_with_specialists_first() {
        let mut a = app();
        a.cat = tab(&a, "Recon");
        let recon = names(&a);
        assert!(recon.contains(&"nmap".to_string())); // network first, recon second
                                                      // tools whose main category is recon come before the others
        let first_secondary = recon
            .iter()
            .position(|n| {
                let t = a.tools.iter().find(|t| &t.name == n).unwrap();
                t.primary() != "recon"
            })
            .unwrap();
        assert!(recon[first_secondary..]
            .iter()
            .all(|n| { a.tools.iter().find(|t| &t.name == n).unwrap().primary() != "recon" }));
        a.cat = tab(&a, "Network");
        assert_eq!(names(&a)[0], "nmap");
    }

    #[test]
    fn filter_works_inside_a_tab_and_matches_category_labels() {
        let mut a = app();
        a.cat = tab(&a, "Web");
        a.set_filter("brute".into());
        let n = names(&a);
        assert!(n.contains(&"dirsearch".to_string()), "{n:?}");
        assert!(!n.contains(&"nmap".to_string()));
        a.cat = 0;
        a.set_filter("mobile".into()); // a category label, not in any description
        assert!(names(&a).contains(&"jadx".to_string()));
    }

    #[test]
    fn switching_category_keeps_the_selection_in_view() {
        let mut a = app();
        a.selected = a.tools.iter().position(|t| t.name == "nmap").unwrap();
        a.cat = tab(&a, "Recon") - 1;
        a.step_category(1); // to Recon: nmap is there too
        assert_eq!(a.tools[a.selected].name, "nmap");
        a.step_category(1); // Network: still there
        assert_eq!(a.tools[a.selected].name, "nmap");
        a.cat = tab(&a, "Stego");
        a.keep_selection_visible();
        assert!(a.visible().contains(&a.selected));
        // wraps around both ways
        a.cat = 0;
        a.step_category(-1);
        assert_eq!(a.cat, a.cat_tabs.len() - 1);
    }

    #[test]
    fn slots_fill_then_reuse_the_oldest_finished() {
        let mut a = app();
        assert_eq!(a.free_slot(), Some(0));
        fake(&mut a, 0, "a", true, 0);
        fake(&mut a, 1, "b", false, 9);
        assert_eq!(a.free_slot(), Some(2)); // empty beats finished
        fake(&mut a, 2, "c", false, 3);
        fake(&mut a, 3, "d", true, 0);
        assert_eq!(a.free_slot(), Some(2)); // c finished before b
        fake(&mut a, 1, "b", true, 0);
        fake(&mut a, 2, "c", true, 0);
        assert_eq!(a.free_slot(), None); // all busy: ask
    }

    #[test]
    fn all_busy_asks_which_to_replace() {
        let mut a = app();
        for i in 0..SLOTS {
            fake(&mut a, i, "busy", true, 0);
        }
        let nmap = a.tools.iter().position(|t| t.name == "nmap").unwrap();
        a.run_in_slot(nmap, vec![]);
        assert_eq!(a.mode, Mode::SlotPick);
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.mode, Mode::Browsing);
        assert!(a.pending.is_none());
        assert!(a.slots.iter().flatten().all(|s| s.title == "busy"));
    }

    #[test]
    fn closing_a_running_terminal_needs_a_second_x() {
        let mut a = app();
        fake(&mut a, 0, "nmap", true, 0);
        fake(&mut a, 2, "ffuf", false, 1);
        a.set_focus(1);
        a.on_key(key(KeyCode::Char('x')));
        assert!(a.slots[0].is_some(), "first x only asks");
        assert_eq!(a.confirm, Some(Confirm::Close(1)));
        a.on_key(key(KeyCode::Char('x')));
        assert!(a.slots[0].is_none());
        assert_eq!(a.focus, 3, "moves to the terminal that's left");
        // a finished one closes at once; then only the console is left
        a.on_key(key(KeyCode::Char('x')));
        assert!(a.slots[2].is_none());
        assert_eq!(a.focus, 0);
        // another key in between cancels the confirmation
        fake(&mut a, 0, "nmap", true, 0);
        a.set_focus(1);
        a.on_key(key(KeyCode::Char('x')));
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Char('x')));
        assert!(a.slots[0].is_some());
    }

    #[test]
    fn quitting_with_tools_running_needs_a_second_q() {
        let mut a = app();
        fake(&mut a, 1, "nmap", true, 0);
        a.on_key(key(KeyCode::Char('q')));
        assert!(!a.quit);
        a.on_key(key(KeyCode::Char('q')));
        assert!(a.quit);
    }

    #[test]
    fn output_goes_to_its_own_terminal_and_marks_unread() {
        let mut a = app();
        fake(&mut a, 0, "one", true, 0);
        fake(&mut a, 1, "two", true, 0);
        let (one, two) = (
            a.slots[0].as_ref().unwrap().id,
            a.slots[1].as_ref().unwrap().id,
        );
        a.set_focus(1);
        a.on_runner_event(AppEvent::Output {
            sid: one,
            line: "a".into(),
        });
        a.on_runner_event(AppEvent::Output {
            sid: two,
            line: "b".into(),
        });
        assert_eq!(a.slots[0].as_ref().unwrap().output, ["a"]);
        assert_eq!(a.slots[1].as_ref().unwrap().output, ["b"]);
        assert!(!a.slots[0].as_ref().unwrap().unread);
        assert!(a.slots[1].as_ref().unwrap().unread);
        a.on_runner_event(AppEvent::Exited {
            sid: two,
            code: Some(0),
        });
        assert!(a.status_line.contains("two finished in terminal 2"));
        a.on_key(key(KeyCode::Char('2')));
        assert!(!a.slots[1].as_ref().unwrap().unread);
        // a closed terminal's late output is dropped, not misrouted
        a.slots[0] = None;
        a.on_runner_event(AppEvent::Output {
            sid: one,
            line: "late".into(),
        });
        assert!(a
            .slots
            .iter()
            .flatten()
            .all(|s| !s.output.contains(&"late".into())));
    }

    #[test]
    fn tab_cycles_through_open_terminals_and_grid_skips_the_console() {
        let mut a = app();
        fake(&mut a, 1, "x", true, 0);
        fake(&mut a, 3, "y", true, 0);
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.focus, 2);
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.focus, 4);
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.focus, 0);
        a.on_key(key(KeyCode::Char('g')));
        assert!(a.grid);
        assert_eq!(a.focus, 2);
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.focus, 4, "grid never focuses the console");
        a.on_key(key(KeyCode::Char('0')));
        assert!(!a.grid);
    }
}
