//! Application state and the main event loop.

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
use crate::registry::{Catalog, Install, Launch, Tool};
use crate::runner::{self, AppEvent};
use crate::scoop;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Unknown,
    Installed,
    Missing,
    /// the exe on PATH failed its `verify` check (a same-named program)
    Conflict,
}

#[derive(PartialEq, Eq)]
pub enum Mode {
    Browsing,
    /// editing the selected tool's arguments
    Input,
    /// typing a hash for the built-in identifier
    HashInput,
    /// typing a line for the running process's stdin
    StdinInput,
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
    pub output: Vec<String>,
    pub mode: Mode,
    pub editor: LineEditor,
    history: History,
    pub running: bool,
    pub running_pid: Option<u32>,
    /// stdin of the running tool (None for installs, or after Ctrl+D)
    proc_stdin: Option<std::process::ChildStdin>,
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
            ":: Enter runs the highlighted tool · i installs it (scoop or pipx).".into(),
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
            conflicts: vec![None; n],
            selected: 0,
            output,
            mode: Mode::Browsing,
            editor: LineEditor::default(),
            history: History::default(),
            running: false,
            running_pid: None,
            proc_stdin: None,
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
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => match self.screen {
                        Screen::Splash => self.screen = Screen::Main,
                        Screen::Main => self.on_key(key),
                    },
                    // bracketed paste: the whole pasted text arrives at once
                    Event::Paste(text) if self.mode != Mode::Browsing => {
                        self.editor.insert_str(&text)
                    }
                    _ => {}
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
        let probes: Vec<(String, Option<detect::Verify>)> = self
            .tools
            .iter()
            .map(|t| (t.exe.clone(), t.verify.clone()))
            .collect();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if refresh_path {
                for dir in envpath::refresh() {
                    let _ = tx.send(AppEvent::Output(format!(":: PATH: picked up {dir}")));
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
                self.proc_stdin = None;
                if self.mode == Mode::StdinInput {
                    self.mode = Mode::Browsing;
                    self.editor.set("");
                }
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

    fn on_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Browsing => self.on_key_browsing(key.code),
            Mode::Input | Mode::HashInput | Mode::StdinInput => self.on_key_input(key),
        }
    }

    /// History key for the open prompt: the tool's name, or the hash prompt.
    fn history_key(&self) -> String {
        if self.mode == Mode::HashInput {
            "\u{0}hashid".to_string()
        } else if self.mode == Mode::StdinInput {
            "\u{0}stdin".to_string()
        } else {
            self.tools[self.selected].name.clone()
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
            KeyCode::Char('h') => {
                self.editor.set("");
                self.history.reset();
                self.mode = Mode::HashInput;
            }
            KeyCode::Enter => {
                if self.tools[self.selected].launch == Launch::Dataset {
                    // nothing to run, so no args prompt
                    self.show_dataset();
                } else if self.running {
                    if self.proc_stdin.is_some() {
                        // talk to the running tool (answer a prompt, etc.)
                        self.editor.set("");
                        self.history.reset();
                        self.mode = Mode::StdinInput;
                    } else {
                        self.status_line =
                            "a process is already running (press c to cancel)".into();
                    }
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

    fn on_key_input(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let e = &mut self.editor;
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browsing;
                self.editor.set("");
                self.history.reset();
            }
            KeyCode::Enter => {
                let text = self.editor.take();
                let hkey = self.history_key();
                self.history.push(&hkey, &text);
                let mode = std::mem::replace(&mut self.mode, Mode::Browsing);
                match mode {
                    Mode::HashInput => self.identify_hash(&text),
                    Mode::StdinInput => self.send_stdin(&text),
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
                self.proc_stdin = None;
                self.mode = Mode::Browsing;
                self.editor.set("");
                self.push(":: closed the tool's input (EOF)");
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
    }

    /// Send one line to the running tool's stdin (echoed in the pane).
    fn send_stdin(&mut self, line: &str) {
        use std::io::Write;
        let Some(stdin) = self.proc_stdin.as_mut() else {
            self.push("[err] the tool's input is closed");
            return;
        };
        let res = stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush());
        match res {
            Ok(()) => self.push(format!("> {line}")),
            Err(e) => {
                self.proc_stdin = None;
                self.push(format!("[err] couldn't send input: {e}"));
            }
        }
    }

    /// Built-in hash identifier (`h`): no install, works offline.
    fn identify_hash(&mut self, hash: &str) {
        if hash.trim().is_empty() {
            return;
        }
        self.push(format!("$ hashid {}", hash.trim()));
        for line in hashid::report(hash) {
            self.push(line);
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
        if status == ToolStatus::Conflict {
            let path = self.conflicts[self.selected]
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            self.push(format!(
                "[err] the '{}' on PATH ({path}) isn't {name} — a different program with \
                 the same name comes first. Not running it.",
                self.tools[self.selected].exe
            ));
            self.push(
                "[err] fix: press i to install the right one, or move its folder earlier in PATH \
                 (or uninstall the other program), then press r.",
            );
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
        let tool = self.tools[self.selected].clone();
        if let Some(note) = &tool.note {
            self.push(format!(":: note: {note}"));
        }

        match tool.launch {
            Launch::Stream => {
                self.start_streaming(&name, &cmd, &args);
            }
            Launch::Gui => {
                self.push(format!("$ {} {}", cmd, args::join(&args)));
                match runner::spawn_detached(runner::tool_command(&cmd, &args)) {
                    Ok(pid) => {
                        self.push(format!(
                            "── launched {name} (pid {pid}); it runs in its own window ──"
                        ));
                        self.status_line = format!("launched {name}");
                    }
                    Err(e) => self.push(format!("failed to launch: {e}")),
                }
            }
            Launch::Web => {
                let url = tool.url.clone().unwrap_or_default();
                if cmd.trim().is_empty() {
                    self.open_url(&url, 0);
                } else {
                    // start the server (streamed, cancellable), then give it
                    // a moment before opening the browser
                    if self.start_streaming(&name, &cmd, &args) {
                        self.open_url(&url, 2);
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
                self.push(format!(
                    "'{}' is not installed — press i to install it first.",
                    tool.name
                ));
                return;
            }
        }
        let Some(dir) = std::path::Path::new(&tool.exe)
            .parent()
            .map(|d| d.to_path_buf())
        else {
            return;
        };
        self.push(format!(":: {} — {}", tool.name, dir.display()));
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
            self.push(format!("   {n:<w$}  {:>8.1} KB", size as f64 / 1024.0));
        }
        match runner::copy_to_clipboard(&dir.to_string_lossy()) {
            Ok(()) => self.push(":: folder path copied to the clipboard"),
            Err(e) => self.push(format!("[err] couldn't copy the path: {e}")),
        }
    }

    /// Run `cmd args` with output streamed into the pane. Returns whether it
    /// started.
    fn start_streaming(&mut self, name: &str, cmd: &str, args: &[String]) -> bool {
        self.push(format!("$ {} {}", cmd, args::join(args)));
        match runner::spawn_streaming(runner::tool_command(cmd, args), self.tx.clone(), true) {
            Ok(sp) => {
                let pid = sp.pid;
                self.proc_stdin = sp.stdin;
                self.running = true;
                self.running_pid = Some(pid);
                self.status_line = format!("running {name} (pid {pid})");
                true
            }
            Err(e) => {
                self.push(format!("failed to launch: {e}"));
                false
            }
        }
    }

    /// Open `url` in the default browser after `delay_s` seconds.
    fn open_url(&mut self, url: &str, delay_s: u64) {
        self.push(format!(":: opening {url} in your browser"));
        let url = url.to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(delay_s));
            if let Err(e) = runner::open_url(&url) {
                let _ = tx.send(AppEvent::Output(format!("[err] couldn't open {url}: {e}")));
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

    fn install_selected(&mut self) {
        if self.running {
            self.status_line = "wait for the current process to finish".into();
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
        let steps = match install::plan(&tool, pre, &self.buckets) {
            Ok(s) => s,
            Err(e) => {
                self.push(format!("[err] can't install {name}: {e}"));
                return;
            }
        };
        let line = steps.join(" && ");
        self.push(format!("$ {line}"));

        // scoop and pipx are .cmd/.bat shims, so go through cmd. Every token
        // in `line` was checked by install::plan to be free of cmd
        // metacharacters, and `&&` stops at the first failing step. /D skips
        // any AutoRun commands from the registry.
        let mut cmd = std::process::Command::new("cmd");
        {
            use std::os::windows::process::CommandExt;
            cmd.raw_arg(format!("/D /C {line}"));
        }
        match runner::spawn_streaming(cmd, self.tx.clone(), false) {
            Ok(sp) => {
                self.running = true;
                self.running_pid = Some(sp.pid);
                self.installing = true;
                self.status_line = format!("installing {name}…");
            }
            Err(e) => self.push(format!("failed to start the installer: {e}")),
        }
    }

    fn cancel(&mut self) {
        self.proc_stdin = None;
        if let Some(pid) = self.running_pid {
            runner::kill_tree(pid);
            self.push(format!("sent kill to pid {pid}"));
        } else {
            self.status_line = "nothing is running".into();
        }
    }
}
