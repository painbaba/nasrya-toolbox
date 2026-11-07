//! Process runner: spawns an external tool with piped stdout/stderr and
//! streams every line back to the UI over a channel, without blocking the
//! TUI event loop. Also supports killing the process tree (tools spawn
//! grandchildren, so a plain child.kill() is not enough on Windows).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use crate::detect;

/// Messages sent from runner threads into the app event loop.
pub enum AppEvent {
    /// One line of tool output (stderr lines are prefixed with "[err] ").
    Output(String),
    /// The process finished with this exit code (None if it was killed/unknown).
    Exited(Option<i32>),
    /// Background detection finished: one installed/missing flag per tool.
    Detected(Vec<bool>),
}

/// Build the command that runs a tool: `program` resolved the way Windows
/// would (see `resolve_program`) plus its arguments.
pub fn tool_command(program: &str, args: &[String]) -> Command {
    let mut cmd = Command::new(resolve_program(program));
    cmd.args(args);
    cmd
}

/// Spawn `cmd`, streaming its output to `tx`. Returns the child PID.
pub fn spawn_streaming(mut cmd: Command, tx: Sender<AppEvent>) -> std::io::Result<u32> {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    let pid = child.id();

    // stdout reader thread
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(out);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(AppEvent::Output(l)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // stderr reader thread (distinguished so the UI can colour it)
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(err);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(AppEvent::Output(format!("[err] {l}"))).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // waiter thread: owns the child so we can reap it; we keep only the PID.
    thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        let _ = tx.send(AppEvent::Exited(code));
    });

    Ok(pid)
}

/// Resolve a bare command name to the file Windows would actually run.
///
/// `Command::new("jadx")` only finds `jadx.exe`; it never tries `.cmd`/`.bat`,
/// and many Scoop shims (jadx, apktool, Python/Java wrappers) are exactly that.
/// So we resolve it with the same PATH x PATHEXT search detection uses
/// (`detect::resolve`), which also never looks in the current directory, and
/// launch the full path. std's `Command` runs a resolved `.bat`/`.cmd` through
/// cmd.exe with safe argument escaping. Falls back to the bare name.
fn resolve_program(program: &str) -> PathBuf {
    detect::resolve(program).unwrap_or_else(|| PathBuf::from(program))
}

/// Start a desktop app and let it go: no pipes, nothing to wait on.
/// CREATE_NO_WINDOW keeps `.bat` launchers (Ghidra, jadx-gui, ...) from
/// leaving an empty console window behind; GUI windows still appear.
pub fn spawn_detached(mut cmd: Command) -> std::io::Result<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(child.id())
}

/// Open an http(s) URL, or an existing local .html page, in the default
/// browser. Goes through `url.dll,FileProtocolHandler` rather than
/// `cmd /C start`, so characters like `&` are never interpreted by a shell.
pub fn open_url(url: &str) -> std::io::Result<()> {
    let web = url.starts_with("http://") || url.starts_with("https://");
    let lower = url.to_ascii_lowercase();
    let page = (lower.ends_with(".html") || lower.ends_with(".htm"))
        && std::path::Path::new(url).is_absolute()
        && std::path::Path::new(url).is_file();
    if !(web || page) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only http(s) URLs and existing local .html pages are opened",
        ));
    }
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
}

/// Kill a process and its whole tree (taskkill /T). Best-effort.
pub fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
