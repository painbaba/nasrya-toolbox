//! Application state and the main event loop.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::args;
use crate::banner;
use crate::detect;
use crate::envpath;
use crate::registry::{Catalog, Tool};
use crate::runner::{self, AppEvent};
use crate::scoop;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Unknown,
    Installed,
    Missing,
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Browsing,
    Input,
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
    pub selected: usize,
    pub output: Vec<String>,
    pub mode: Mode,
    pub input: String,
    pub running: bool,
    pub running_pid: Option<u32>,
    pub status_line: String,
    pub tick: u64,
    /// A background detection pass is in flight.
    detecting: bool,
    /// The running process is a scoop install (so re-detect when it exits).
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
        let mut output = vec![
            ":: nasrya ready. tools above; output appears here.".into(),
            ":: Enter runs the highlighted tool · i installs it via scoop.".into(),
            format!(":: catalog: {}", catalog_path.display()),
        ];
        if let Some(ignored) = ignored {
            output.push(format!(
                "[err] ignored {} — catalogs aren't loaded from the current directory \
                 unless you pass --catalog",
                ignored.display()
            ));
        }
        App {
            tools,
            buckets,
            status: vec![ToolStatus::Unknown; n],
            selected: 0,
            output,
            mode: Mode::Browsing,
            input: String::new(),
            running: false,
            running_pid: None,
            status_line: "detecting tools…".into(),
            tick: 0,
            detecting: false,
            installing: false,
            screen: Screen::Splash,
            tx,
            rx,
            quit: false,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.detect_all(false);
        while !self.quit {
            terminal.draw(|f| match self.screen {
                Screen::Splash => banner::render_splash(f, self.tick),
                Screen::Main => ui::render(f, &self),
            })?;

            // drain runner events
            while let Ok(ev) = self.rx.try_recv() {
                self.on_runner_event(ev);
            }

            if event::poll(Duration::from_millis(80))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match self.screen {
                            Screen::Splash => self.screen = Screen::Main,
                            Screen::Main => self.on_key(key.code),
                        }
                    }
                }
            }
            self.tick = self.tick.wrapping_add(1);
        }
        Ok(())
    }

    /// Re-probe every tool on a background thread; the result comes back as
    /// `AppEvent::Detected`, so the UI keeps drawing meanwhile. With
    /// `refresh_path`, first pick up PATH changes persisted since nasrya
    /// started (installs that add a PATH entry, a freshly installed Scoop).
    fn detect_all(&mut self, refresh_path: bool) {
        if self.detecting {
            return;
        }
        self.detecting = true;
        self.status_line = "detecting tools…".into();
        let exes: Vec<String> = self.tools.iter().map(|t| t.exe.clone()).collect();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if refresh_path {
                for dir in envpath::refresh() {
                    let _ = tx.send(AppEvent::Output(format!(":: PATH: picked up {dir}")));
                }
            }
            let found = detect::installed_all(&exes);
            let _ = tx.send(AppEvent::Detected(found));
        });
    }

    fn on_detected(&mut self, found: Vec<bool>) {
        self.detecting = false;
        self.status = found
            .into_iter()
            .map(|ok| {
                if ok {
                    ToolStatus::Installed
                } else {
                    ToolStatus::Missing
                }
            })
            .collect();
        let installed = self
            .status
            .iter()
            .filter(|s| **s == ToolStatus::Installed)
            .count();
        self.status_line = format!("{installed}/{} tools installed", self.tools.len());
    }

    fn push(&mut self, line: impl Into<String>) {
        self.output.push(line.into());
        const MAX: usize = 2000;
        if self.output.len() > MAX {
            let excess = self.output.len() - MAX;
            self.output.drain(0..excess);
        }
    }

    fn on_runner_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Output(l) => self.push(l),
            AppEvent::Exited(code) => {
                self.running = false;
                self.running_pid = None;
                match code {
                    Some(c) => self.push(format!("── process exited (code {c}) ──")),
                    None => self.push("── process ended ──".to_string()),
                }
                // Only an install can change what's on PATH; a plain tool run
                // doesn't need a re-probe.
                if std::mem::take(&mut self.installing) {
                    self.detect_all(true);
                }
            }
            AppEvent::Detected(found) => self.on_detected(found),
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match self.mode {
            Mode::Browsing => self.on_key_browsing(code),
            Mode::Input => self.on_key_input(code),
        }
    }

    fn on_key_browsing(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.tools.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Char('r') => self.detect_all(true),
            KeyCode::Char('i') => self.install_selected(),
            KeyCode::Char('c') => self.cancel(),
            KeyCode::Enter => {
                if self.running {
                    self.status_line = "a process is already running (press c to cancel)".into();
                } else {
                    let hint = self.tools[self.selected].args_hint.clone();
                    self.input = hint.unwrap_or_default();
                    self.mode = Mode::Input;
                }
            }
            _ => {}
        }
    }

    fn on_key_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.mode = Mode::Browsing;
                self.input.clear();
            }
            KeyCode::Enter => {
                let args = self.input.clone();
                self.mode = Mode::Browsing;
                self.input.clear();
                self.launch(&args);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn launch(&mut self, args_str: &str) {
        let status = self.status[self.selected];
        let name = self.tools[self.selected].name.clone();
        let cmd = self.tools[self.selected].run_cmd.clone();

        if status == ToolStatus::Unknown {
            self.status_line = "still detecting tools — try again in a moment".into();
            return;
        }
        if status != ToolStatus::Installed {
            self.push(format!(
                "'{name}' is not installed — press i to install it first."
            ));
            return;
        }

        let args = match args::split(args_str) {
            Ok(a) => a,
            Err(e) => {
                self.push(format!("[err] can't parse args: {e} — nothing was run."));
                return;
            }
        };
        self.push(format!("$ {} {}", cmd, args::join(&args)));

        match runner::spawn_streaming(runner::tool_command(&cmd, &args), self.tx.clone()) {
            Ok(pid) => {
                self.running = true;
                self.running_pid = Some(pid);
                self.status_line = format!("running {name} (pid {pid})");
            }
            Err(e) => self.push(format!("failed to launch: {e}")),
        }
    }

    fn install_selected(&mut self) {
        if self.running {
            self.status_line = "wait for the current process to finish".into();
            return;
        }
        let pkg = self.tools[self.selected].scoop.clone();
        let name = self.tools[self.selected].name.clone();
        let bucket = self.tools[self.selected].bucket.clone();

        if detect::resolve("scoop").is_none() {
            self.push("[err] scoop isn't installed (or not on PATH). Install it first:");
            self.push("[err]   irm get.scoop.sh | iex      (in PowerShell; see https://scoop.sh)");
            return;
        }

        let added = bucket.as_deref().is_some_and(scoop::bucket_added);
        let steps = match scoop::plan_install(&pkg, bucket.as_deref(), added, &self.buckets) {
            Ok(s) => s,
            Err(e) => {
                self.push(format!("[err] can't install {name}: {e}"));
                return;
            }
        };
        let line = steps.join(" && ");
        self.push(format!("$ {line}"));

        // scoop is a .cmd/.ps1 shim, so go through cmd. Every token in `line`
        // was checked by plan_install to be free of cmd metacharacters, and
        // `&&` stops the install if adding the bucket failed. /D skips any
        // AutoRun commands from the registry.
        let mut cmd = std::process::Command::new("cmd");
        {
            use std::os::windows::process::CommandExt;
            cmd.raw_arg(format!("/D /C {line}"));
        }
        match runner::spawn_streaming(cmd, self.tx.clone()) {
            Ok(pid) => {
                self.running = true;
                self.running_pid = Some(pid);
                self.installing = true;
                self.status_line = format!("installing {name}…");
            }
            Err(e) => self.push(format!("failed to start scoop: {e}")),
        }
    }

    fn cancel(&mut self) {
        if let Some(pid) = self.running_pid {
            runner::kill_tree(pid);
            self.push(format!("sent kill to pid {pid}"));
        } else {
            self.status_line = "nothing is running".into();
        }
    }
}
