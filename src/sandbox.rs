//! Run a tool inside Windows Sandbox: a throwaway Hyper-V VM that Windows
//! wipes when its window closes. Built for CTF reversing and pwn, where the
//! binary under test is untrusted and a debugger runs it for real.
//!
//! What the VM gets, and nothing else:
//! - the tool's Scoop folder and any runtimes it needs, read-only (Scoop
//!   apps are portable, so nothing is installed inside the VM);
//! - the sample (its folder), read-only;
//! - one fresh output folder per run, the only writable mapping.
//!
//! Networking, clipboard, the virtual GPU, audio, camera and printers are
//! off; networking can be switched on per run. This raises the bar a lot for
//! a hostile crackme, but it is not a malware lab: real malware deserves a
//! dedicated VM with snapshots.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// The account Windows Sandbox logs in as.
const GUEST_HOME: &str = r"C:\Users\WDAGUtilityAccount";
const GUEST_TOOL: &str = r"C:\winsploit\tool";
const GUEST_DEPS: &str = r"C:\winsploit\deps";

pub fn guest_samples() -> String {
    format!(r"{GUEST_HOME}\Desktop\samples")
}

pub fn guest_out() -> String {
    format!(r"{GUEST_HOME}\Desktop\out")
}

/// What to tell someone whose Windows Sandbox isn't available.
pub const ENABLE_HINT: [&str; 3] = [
    "Windows Sandbox isn't enabled. It needs Windows 10/11 Pro, Enterprise or Education \
     (not Home) with virtualization turned on.",
    "Enable it from an admin PowerShell, then reboot:",
    "  Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All",
];

/// `WindowsSandbox.exe`, when the feature is enabled.
pub fn sandbox_exe() -> Option<PathBuf> {
    let win = std::env::var_os("windir").or_else(|| std::env::var_os("SystemRoot"))?;
    let p = PathBuf::from(win)
        .join("System32")
        .join("WindowsSandbox.exe");
    p.is_file().then_some(p)
}

/// One host folder made visible inside the VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub host: PathBuf,
    pub guest: String,
    pub writable: bool,
}

/// What to start, and with what, before it becomes a `.wsb` file.
#[derive(Debug)]
pub struct Request {
    /// The tool's Scoop app folder on the host (a real folder, not a junction).
    pub app_dir: PathBuf,
    /// The program (or `.bat` launcher) to start, relative to `app_dir`.
    pub exe_rel: PathBuf,
    /// A command-line tool: start it in a console window inside the VM.
    pub console: bool,
    /// Runtimes to map read-only and put on PATH: (scoop package, host folder).
    pub deps: Vec<(String, PathBuf)>,
    /// The sample: a file (passed to the tool) or a folder (opened in).
    pub target: Option<PathBuf>,
    /// This run's output folder, the only writable mapping.
    pub out: PathBuf,
    pub network: bool,
}

/// A checked request: every mapping and the command the VM runs at logon.
#[derive(Debug)]
pub struct Plan {
    pub mappings: Vec<Mapping>,
    pub command: String,
    pub network: bool,
}

/// Characters that would let a file or folder name break out of the quoted
/// arguments of the logon `cmd.exe` line.
fn cmd_safe(s: &str) -> bool {
    !s.chars()
        .any(|c| c.is_control() || matches!(c, '"' | '%' | '&' | '|' | '<' | '>' | '^' | '!'))
}

/// Lowercase, `/` -> `\`, no trailing `\`: for comparing Windows paths.
fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
    s.trim_end_matches('\\').to_string()
}

/// Is `inner` the same as `outer` or somewhere inside it?
fn within(inner: &str, outer: &str) -> bool {
    inner == outer || inner.starts_with(&format!("{outer}\\"))
}

/// A host folder the sample folder must not be (or contain). Unless
/// `inside_ok`, the sample folder can't sit inside it either.
#[derive(Debug, Clone)]
pub struct Protected {
    pub path: PathBuf,
    pub inside_ok: bool,
}

/// Mapping any of these would hand the VM your profile, your tools or
/// Windows itself. A folder *inside* your profile (Desktop\ctf\chall) is
/// fine; the profile itself is not.
pub fn protected_dirs() -> Vec<Protected> {
    let var = |k: &str, inside_ok| {
        std::env::var_os(k).map(|v| Protected {
            path: PathBuf::from(v),
            inside_ok,
        })
    };
    let mut v: Vec<Protected> = [
        var("USERPROFILE", true),
        var("APPDATA", false),
        var("LOCALAPPDATA", false),
        var("windir", false),
        var("ProgramFiles", false),
    ]
    .into_iter()
    .flatten()
    .collect();
    v.extend(crate::scoop::scoop_root().map(|path| Protected {
        path,
        inside_ok: false,
    }));
    v
}

/// Refuse sample folders that would expose too much of the host.
pub fn check_sample_dir(dir: &Path, protected: &[Protected]) -> Result<()> {
    if !dir.is_absolute() {
        bail!("use a full path for the sample ({})", dir.display());
    }
    let d = norm(dir);
    if !d.contains('\\') {
        bail!(
            "{} is a whole drive; put the sample in its own folder",
            dir.display()
        );
    }
    for p in protected {
        let pp = norm(&p.path);
        if pp.is_empty() {
            continue;
        }
        if within(&pp, &d) || (!p.inside_ok && within(&d, &pp)) {
            bail!(
                "{} is too broad to share with the sandbox; put the sample in its own folder",
                dir.display()
            );
        }
    }
    Ok(())
}

/// Check a request and turn it into mappings and a logon command.
pub fn plan(r: &Request, protected: &[Protected]) -> Result<Plan> {
    let exe_rel = r.exe_rel.to_string_lossy().to_string();
    if exe_rel.is_empty() || r.exe_rel.is_absolute() || exe_rel.contains("..") {
        bail!("the tool's program path must be inside its own folder");
    }
    if !cmd_safe(&exe_rel) {
        bail!("the tool's path has characters the sandbox can't pass safely");
    }

    let mut mappings = vec![Mapping {
        host: r.app_dir.clone(),
        guest: GUEST_TOOL.to_string(),
        writable: false,
    }];
    let mut env = String::new();
    for (name, host) in &r.deps {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            bail!("bad sandbox dependency name {name:?}");
        }
        let guest = format!(r"{GUEST_DEPS}\{name}");
        env.push_str(&format!(r#"set "PATH={guest}\bin;{guest};%PATH%" && "#));
        if name.contains("jdk") || name.contains("jre") {
            env.push_str(&format!(r#"set "JAVA_HOME={guest}" && "#));
        }
        mappings.push(Mapping {
            host: host.clone(),
            guest,
            writable: false,
        });
    }

    // the sample: map its folder, pass the file (if it is one) to the tool
    let mut arg = None;
    let mut workdir = guest_out();
    if let Some(t) = &r.target {
        let (dir, file) = if t.is_file() {
            let dir = t
                .parent()
                .context("the sample has no folder")?
                .to_path_buf();
            (dir, t.file_name().map(|f| f.to_string_lossy().to_string()))
        } else if t.is_dir() {
            (t.clone(), None)
        } else {
            bail!("{} doesn't exist", t.display());
        };
        check_sample_dir(&dir, protected)?;
        if let Some(f) = &file {
            if !cmd_safe(f) {
                bail!("rename the sample: {f:?} has characters like \" % & | that can't be passed safely");
            }
            arg = Some(format!(r"{}\{f}", guest_samples()));
        }
        workdir = guest_samples();
        mappings.push(Mapping {
            host: dir,
            guest: guest_samples(),
            writable: false,
        });
    }
    mappings.push(Mapping {
        host: r.out.clone(),
        guest: guest_out(),
        writable: true,
    });

    let program = format!(r#""{GUEST_TOOL}\{exe_rel}""#);
    let mut line = program;
    if let Some(a) = arg {
        line.push_str(&format!(r#" "{a}""#));
    }
    // a console tool gets its own window that stays open (`cmd /k`); the
    // outer quotes are the ones cmd strips, the inner ones survive
    let start = if r.console {
        format!(r#"start "" cmd.exe /k "{line}""#)
    } else {
        format!(r#"start "" {line}"#)
    };
    let command = format!(r#"cmd.exe /c {env}cd /d "{workdir}" && {start}"#);

    Ok(Plan {
        mappings,
        command,
        network: r.network,
    })
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The `.wsb` configuration for a plan: everything optional switched off.
pub fn wsb(p: &Plan) -> String {
    let mut s = String::from("<Configuration>\n");
    let net = if p.network { "Default" } else { "Disable" };
    for (k, v) in [
        ("Networking", net),
        ("vGPU", "Disable"),
        ("AudioInput", "Disable"),
        ("VideoInput", "Disable"),
        ("ClipboardRedirection", "Disable"),
        ("PrinterRedirection", "Disable"),
        ("ProtectedClient", "Enable"),
        ("MemoryInMB", "4096"),
    ] {
        s.push_str(&format!("  <{k}>{v}</{k}>\n"));
    }
    s.push_str("  <MappedFolders>\n");
    for m in &p.mappings {
        let host = m.host.to_string_lossy();
        let host = host.strip_prefix(r"\\?\").unwrap_or(&host);
        s.push_str(&format!(
            "    <MappedFolder>\n      <HostFolder>{}</HostFolder>\n      \
             <SandboxFolder>{}</SandboxFolder>\n      <ReadOnly>{}</ReadOnly>\n    </MappedFolder>\n",
            xml(host),
            xml(&m.guest),
            !m.writable
        ));
    }
    s.push_str("  </MappedFolders>\n");
    s.push_str(&format!(
        "  <LogonCommand>\n    <Command>{}</Command>\n  </LogonCommand>\n",
        xml(&p.command)
    ));
    s.push_str("</Configuration>\n");
    s
}

// ------------------------------------------------------------ finding the exe

/// The program a Scoop shim points at. `.shim` files say `path = "..."`;
/// `.cmd` / `.bat` shims call it in quotes.
pub fn parse_shim(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("path") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix('=') {
                let v = v.trim().trim_matches('"');
                if !v.is_empty() {
                    return Some(PathBuf::from(v));
                }
            }
        }
    }
    // @"C:\...\apps\ghidra\current\ghidraRun.bat" %*
    text.split('"')
        .skip(1)
        .step_by(2)
        .find(|p| p.to_lowercase().contains(r"\apps\"))
        .map(PathBuf::from)
}

/// `target` relative to package `pkg`'s app folder, whichever version folder
/// (`current` or `1.2.3`) the path goes through.
pub fn rel_in_app(target: &Path, pkg: &str) -> Option<PathBuf> {
    let t = target.to_string_lossy().replace('/', "\\");
    let key = format!(r"\apps\{}\", pkg.to_lowercase());
    let at = t.to_lowercase().find(&key)? + key.len();
    let rest = &t[at..];
    let (_version, rel) = rest.split_once('\\')?;
    (!rel.is_empty()).then(|| PathBuf::from(rel))
}

/// Search `dir` (a few levels deep) for `name` with a runnable extension.
fn find_in(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            subdirs.push(path);
            continue;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase());
        let ext = path.extension().map(|s| s.to_string_lossy().to_lowercase());
        if stem.as_deref() == Some(&name.to_lowercase())
            && matches!(ext.as_deref(), Some("exe" | "bat" | "cmd" | "com"))
        {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.sort();
    subdirs.iter().find_map(|d| find_in(d, name, depth - 1))
}

/// A Scoop app folder with the junction resolved, without the `\\?\` prefix.
pub fn real_dir(p: &Path) -> Result<PathBuf> {
    let c = std::fs::canonicalize(p).with_context(|| format!("{} not found", p.display()))?;
    let s = c.to_string_lossy();
    Ok(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s)))
}

/// Where the tool lives: its real app folder and the program inside it.
pub fn locate(run_cmd: &str, scoop_root: &Path, pkg: &str) -> Result<(PathBuf, PathBuf)> {
    let app = scoop_root.join("apps").join(pkg).join("current");
    if !app.is_dir() {
        bail!("{pkg} isn't installed with Scoop (no {})", app.display());
    }
    let rel = if run_cmd.contains('\\') || run_cmd.contains(':') {
        rel_in_app(Path::new(run_cmd), pkg)
    } else {
        ["shim", "cmd", "bat"]
            .iter()
            .map(|ext| scoop_root.join("shims").join(format!("{run_cmd}.{ext}")))
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .find_map(|text| parse_shim(&text))
            .and_then(|t| rel_in_app(&t, pkg))
            .or_else(|| {
                find_in(&app, run_cmd, 3)
                    .and_then(|p| p.strip_prefix(&app).ok().map(Path::to_path_buf))
            })
    };
    let rel = rel.with_context(|| format!("couldn't find {run_cmd} inside {}", app.display()))?;
    Ok((real_dir(&app)?, rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join("winsploit-sandbox-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn req(target: Option<PathBuf>) -> Request {
        Request {
            app_dir: PathBuf::from(r"C:\s\apps\x64dbg\2025.01"),
            exe_rel: PathBuf::from(r"release\x64\x64dbg.exe"),
            console: false,
            deps: vec![],
            target,
            out: PathBuf::from(r"C:\out & co\run1"),
            network: false,
        }
    }

    #[test]
    fn only_the_output_folder_is_writable_and_network_is_off() {
        let dir = tmp("plan");
        let sample = dir.join("crackme.exe");
        std::fs::write(&sample, b"MZ").unwrap();
        let p = plan(&req(Some(sample)), &[]).unwrap();
        let writable: Vec<_> = p.mappings.iter().filter(|m| m.writable).collect();
        assert_eq!(writable.len(), 1);
        assert_eq!(writable[0].guest, guest_out());
        let x = wsb(&p);
        assert!(x.contains("<Networking>Disable</Networking>"), "{x}");
        assert!(x.contains("<ClipboardRedirection>Disable</ClipboardRedirection>"));
        assert_eq!(x.matches("<ReadOnly>true</ReadOnly>").count(), 2, "{x}");
        // the file is passed to the tool, from the read-only samples folder
        assert!(
            p.command.contains(r"\Desktop\samples\crackme.exe"),
            "{}",
            p.command
        );
        assert!(p
            .command
            .contains(r"C:\winsploit\tool\release\x64\x64dbg.exe"));
        // host paths are XML-escaped
        assert!(x.contains(r"C:\out &amp; co\run1"), "{x}");
        assert!(!x.contains(" & co"));
    }

    #[test]
    fn network_can_be_switched_on() {
        let mut r = req(None);
        r.network = true;
        assert!(wsb(&plan(&r, &[]).unwrap()).contains("<Networking>Default</Networking>"));
    }

    #[test]
    fn console_tools_and_runtimes() {
        let mut r = req(None);
        r.console = true;
        r.exe_rel = PathBuf::from("radare2.exe");
        r.deps = vec![(
            "temurin25-jdk".into(),
            PathBuf::from(r"C:\s\apps\temurin25-jdk\25"),
        )];
        let p = plan(&r, &[]).unwrap();
        assert!(
            p.command
                .contains(r#"cmd.exe /k ""C:\winsploit\tool\radare2.exe"""#),
            "{}",
            p.command
        );
        assert!(p
            .command
            .contains(r"JAVA_HOME=C:\winsploit\deps\temurin25-jdk"));
        assert!(p
            .mappings
            .iter()
            .any(|m| m.guest.ends_with("temurin25-jdk") && !m.writable));
    }

    #[test]
    fn refuses_unsafe_names_and_broad_folders() {
        let dir = tmp("unsafe");
        let bad = dir.join("a&b.exe");
        std::fs::write(&bad, b"MZ").unwrap();
        assert!(plan(&req(Some(bad)), &[]).is_err());
        let mut r = req(None);
        r.exe_rel = PathBuf::from(r"..\..\Windows\notepad.exe");
        assert!(plan(&r, &[]).is_err());

        let pr = |p: &str, inside_ok| Protected {
            path: PathBuf::from(p),
            inside_ok,
        };
        let prot = vec![
            pr(r"C:\Users\me", true),
            pr(r"C:\Users\me\scoop", false),
            pr(r"C:\Windows", false),
        ];
        for d in [
            r"C:\",
            r"C:\Users",
            r"C:\Users\me",
            r"C:\Users\me\scoop\apps",
            r"C:\Windows\Temp",
        ] {
            assert!(
                check_sample_dir(Path::new(d), &prot).is_err(),
                "{d} was allowed"
            );
        }
        assert!(check_sample_dir(Path::new(r"C:\ctf\chall1"), &prot).is_ok());
        assert!(check_sample_dir(Path::new(r"C:\Users\me\Desktop\ctf"), &prot).is_ok());
    }

    #[test]
    fn shims_and_app_relative_paths() {
        let shim = "path = \"C:\\Users\\me\\scoop\\apps\\x64dbg\\current\\release\\x64\\x64dbg.exe\"\nargs = \n";
        let t = parse_shim(shim).unwrap();
        assert_eq!(
            rel_in_app(&t, "x64dbg"),
            Some(PathBuf::from(r"release\x64\x64dbg.exe"))
        );
        let cmd = "@rem shim\r\n@\"C:\\s\\apps\\ghidra\\11.2\\ghidraRun.bat\" %*\r\n";
        assert_eq!(
            rel_in_app(&parse_shim(cmd).unwrap(), "Ghidra"),
            Some(PathBuf::from("ghidraRun.bat"))
        );
        assert_eq!(rel_in_app(Path::new(r"C:\elsewhere\x.exe"), "x64dbg"), None);
    }

    #[test]
    fn locate_through_a_shim_or_by_searching() {
        let root = tmp("scoop");
        let app = root.join(r"apps\cutter\current\bin");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("cutter.exe"), b"MZ").unwrap();
        // no shim: found by searching the app folder
        let (dir, rel) = locate("cutter", &root, "cutter").unwrap();
        assert!(dir.ends_with(r"apps\cutter\current"), "{}", dir.display());
        assert_eq!(rel, PathBuf::from(r"bin\cutter.exe"));
        // a shim wins
        std::fs::create_dir_all(root.join("shims")).unwrap();
        std::fs::write(
            root.join(r"shims\cutter.shim"),
            format!(
                "path = \"{}\"",
                root.join(r"apps\cutter\current\bin\cutter.exe").display()
            ),
        )
        .unwrap();
        assert_eq!(
            locate("cutter", &root, "cutter").unwrap().1,
            PathBuf::from(r"bin\cutter.exe")
        );
        assert!(locate("cutter", &root, "nope").is_err());
    }
}
