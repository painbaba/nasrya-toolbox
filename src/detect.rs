//! Tool detection: is a tool's executable actually runnable from PATH?
//!
//! This resolves names in-process, as Windows itself would (PATH x PATHEXT),
//! instead of spawning `where.exe` once per tool (~40 ms each, which froze
//! the UI as the catalog grew). Each PATH directory is listed once per pass
//! into a name set, so checking 40 tools costs ~40 directory listings, not
//! thousands of stat calls.
//!
//! Two deliberate differences from `where`:
//! - the current directory is never searched, so a stray `nmap.exe` in the
//!   folder you started nasrya from can't pose as the real tool;
//! - only extensions we can actually launch count (`.exe .com .bat .cmd`), so
//!   Scoop's extensionless sh shims and `.ps1` shims don't produce a false
//!   "installed".

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Extensions `std::process::Command` can launch, in Windows' default order.
const RUNNABLE: &[&str] = &[".com", ".exe", ".bat", ".cmd"];

/// A snapshot of what's on PATH: each directory with its (lowercased) file
/// names, plus the launchable extensions in PATHEXT order.
pub struct PathIndex {
    dirs: Vec<(PathBuf, HashSet<String>)>,
    exts: Vec<String>,
}

impl PathIndex {
    /// Snapshot the current process's PATH / PATHEXT.
    pub fn from_env() -> Self {
        let path = std::env::var("PATH").unwrap_or_default();
        let pathext = std::env::var("PATHEXT").unwrap_or_default();
        Self::build(&path, &pathext, list_dir)
    }

    /// Build from explicit values; `list` returns a directory's file names.
    fn build(
        path_var: &str,
        pathext_var: &str,
        list: impl Fn(&Path) -> Option<Vec<String>>,
    ) -> Self {
        // PATHEXT order wins (that's how Windows picks between x.exe and
        // x.cmd), restricted to what we can launch.
        let mut exts: Vec<String> = pathext_var
            .split(';')
            .map(|e| e.trim().to_ascii_lowercase())
            .filter(|e| RUNNABLE.contains(&e.as_str()))
            .collect();
        if exts.is_empty() {
            exts = RUNNABLE.iter().map(|e| e.to_string()).collect();
        }

        let mut seen = HashSet::new();
        let mut dirs = Vec::new();
        for d in path_var.split(';') {
            let d = d.trim().trim_matches('"');
            if d.is_empty() || !seen.insert(d.to_ascii_lowercase()) {
                continue;
            }
            let dir = PathBuf::from(d);
            if let Some(names) = list(&dir) {
                let names = names.into_iter().map(|n| n.to_ascii_lowercase()).collect();
                dirs.push((dir, names));
            }
        }
        PathIndex { dirs, exts }
    }

    /// Full path of `exe` as it would be launched, or `None`.
    pub fn resolve(&self, exe: &str) -> Option<PathBuf> {
        let given = Path::new(exe);
        if given.components().count() > 1 {
            // An explicit path: no PATH search.
            return given.is_file().then(|| given.to_path_buf());
        }
        let lower = exe.to_ascii_lowercase();
        let has_runnable_ext = RUNNABLE.iter().any(|e| lower.ends_with(e));

        for (dir, names) in &self.dirs {
            if has_runnable_ext {
                if names.contains(&lower) {
                    return Some(dir.join(exe));
                }
                continue;
            }
            for ext in &self.exts {
                let file = format!("{lower}{ext}");
                if names.contains(&file) {
                    return Some(dir.join(format!("{exe}{ext}")));
                }
            }
        }
        None
    }
}

/// File names in `dir`, or `None` if it can't be read.
fn list_dir(dir: &Path) -> Option<Vec<String>> {
    let rd = std::fs::read_dir(dir).ok()?;
    Some(
        rd.filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| !t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
    )
}

/// Resolve one name against the current PATH (builds a fresh snapshot).
pub fn resolve(exe: &str) -> Option<PathBuf> {
    PathIndex::from_env().resolve(exe)
}

/// Is each of `exes` installed? One PATH snapshot for the whole batch.
pub fn installed_all<S: AsRef<str>>(exes: &[S]) -> Vec<bool> {
    let idx = PathIndex::from_env();
    exes.iter()
        .map(|e| idx.resolve(e.as_ref()).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.JS;.PS1";

    /// A fake filesystem: dir -> file names.
    fn idx(path_var: &str, pathext: &str, files: &[(&str, &[&str])]) -> PathIndex {
        let map: HashMap<String, Vec<String>> = files
            .iter()
            .map(|(d, f)| {
                (
                    d.to_ascii_lowercase(),
                    f.iter().map(|s| s.to_string()).collect(),
                )
            })
            .collect();
        PathIndex::build(path_var, pathext, move |p| {
            map.get(&p.to_string_lossy().to_ascii_lowercase()).cloned()
        })
    }

    #[test]
    fn finds_exe_on_path() {
        let i = idx(r"C:\x;C:\a", PATHEXT, &[(r"C:\a", &["nmap.exe"])]);
        assert_eq!(i.resolve("nmap"), Some(PathBuf::from(r"C:\a\nmap.exe")));
    }

    #[test]
    fn finds_cmd_shims_and_skips_sh_and_ps1_shims() {
        // Scoop puts all three next to each other for a .bat-based app.
        let i = idx(
            r"C:\s",
            PATHEXT,
            &[(
                r"C:\s",
                &["jadx", "jadx.ps1", "jadx.cmd", "only", "only.ps1"],
            )],
        );
        assert_eq!(i.resolve("jadx"), Some(PathBuf::from(r"C:\s\jadx.cmd")));
        // a lone sh/ps1 shim does not count as installed
        assert_eq!(i.resolve("only"), None);
    }

    #[test]
    fn earlier_path_dir_wins_then_pathext_order() {
        let i = idx(
            r"C:\a;C:\b",
            PATHEXT,
            &[(r"C:\a", &["t.cmd", "t.exe"]), (r"C:\b", &["t.exe"])],
        );
        assert_eq!(i.resolve("t"), Some(PathBuf::from(r"C:\a\t.exe")));
    }

    #[test]
    fn case_insensitive() {
        let i = idx(r"C:\a", PATHEXT, &[(r"C:\a", &["NMap.EXE"])]);
        assert!(i.resolve("nmap").is_some());
        assert!(i.resolve("NMAP").is_some());
    }

    #[test]
    fn current_directory_is_not_searched() {
        // Nothing is searched implicitly: an empty PATH finds nothing, even
        // though the fake cwd has nmap.exe.
        let i = idx("", PATHEXT, &[(".", &["nmap.exe"])]);
        assert_eq!(i.resolve("nmap"), None);
    }

    #[test]
    fn explicit_runnable_extension() {
        let i = idx(r"C:\a", PATHEXT, &[(r"C:\a", &["x.exe", "y.ps1"])]);
        assert_eq!(i.resolve("x.exe"), Some(PathBuf::from(r"C:\a\x.exe")));
        assert_eq!(i.resolve("y.ps1"), None);
    }

    #[test]
    fn quoted_duplicate_and_empty_path_entries() {
        let i = idx(
            r#";;"C:\Program Files\T";C:\PROGRAM FILES\T;"#,
            PATHEXT,
            &[(r"C:\Program Files\T", &["t.exe"])],
        );
        assert_eq!(
            i.resolve("t"),
            Some(PathBuf::from(r"C:\Program Files\T\t.exe"))
        );
        assert_eq!(i.dirs.len(), 1, "duplicate PATH entry listed once");
    }

    #[test]
    fn missing_pathext_falls_back_to_defaults() {
        let i = idx(r"C:\a", "", &[(r"C:\a", &["t.bat"])]);
        assert_eq!(i.resolve("t"), Some(PathBuf::from(r"C:\a\t.bat")));
    }

    #[test]
    fn unreadable_dirs_are_skipped() {
        let i = idx(r"C:\gone;C:\a", PATHEXT, &[(r"C:\a", &["t.exe"])]);
        assert_eq!(i.resolve("t"), Some(PathBuf::from(r"C:\a\t.exe")));
    }
}
