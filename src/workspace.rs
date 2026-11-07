//! Challenge workspaces: one folder per CTF challenge, grouped by event,
//! under `%USERPROFILE%\ctf`:
//!
//! ```text
//! ctf\picoCTF-2026\rev-crackme1\
//!     challenge.yaml   event, name, target, flag format
//!     files\           challenge files (copies; originals untouched)
//!     logs\            a log per run started while it's active
//!     flags.txt        flags the catcher found
//!     notes.md
//! ```
//!
//! A challenge is addressed by its id, `<event folder>/<challenge folder>`.
//! Ids only ever resolve to folders found by scanning the root, so the web
//! view can switch challenges without ever naming a path.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// `%USERPROFILE%\ctf`.
pub fn root() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ctf")
}

/// What `challenge.yaml` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Info {
    pub event: String,
    pub name: String,
    /// IP, host, URL or `nc host port`: fills `{target}` in recipes.
    pub target: String,
    /// Extra flag prefix for this event, e.g. `ACME` for `ACME{...}`.
    pub flag_format: String,
    /// Wordlist for `{wordlist}` in recipes, when not the default.
    pub wordlist: String,
    pub created: String,
}

/// An open challenge folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub dir: PathBuf,
    /// `<event folder>/<challenge folder>`
    pub id: String,
    pub info: Info,
}

/// A folder-safe version of a name: letters, digits, `-`, `_`, `.`;
/// spaces become `-`; no leading dots; at most 60 characters.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else if c.is_whitespace() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_start_matches('.').trim_matches('-');
    out.chars().take(60).collect()
}

/// `ACME{` / ` acme ` -> `ACME`: the prefix the flag catcher should trust.
pub fn flag_prefix(s: &str) -> String {
    s.trim()
        .trim_end_matches('}')
        .trim_end_matches('{')
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .collect()
}

impl Workspace {
    pub fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.dir.join("logs")
    }
    pub fn flags_path(&self) -> PathBuf {
        self.dir.join("flags.txt")
    }
    pub fn notes_path(&self) -> PathBuf {
        self.dir.join("notes.md")
    }
    /// Where recipes point tools' own output files (`{out}`).
    pub fn out_dir(&self) -> PathBuf {
        self.dir.join("out")
    }

    /// Challenge files, newest first, as full paths.
    pub fn files_newest_first(&self) -> Vec<PathBuf> {
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(self.files_dir())
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file())
                    .map(|p| {
                        let t = std::fs::metadata(&p)
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::UNIX_EPOCH);
                        (t, p)
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        v.into_iter().map(|(_, p)| p).collect()
    }

    /// "picoCTF 2026 / rev crackme1"
    pub fn title(&self) -> String {
        format!("{} / {}", self.info.event, self.info.name)
    }

    /// Make the folders and files for a new challenge (or reopen it if it
    /// already exists, keeping what's there).
    pub fn create(root: &Path, event: &str, name: &str, target: &str, flag: &str) -> Result<Self> {
        let (e, n) = (slug(event), slug(name));
        if e.is_empty() || n.is_empty() {
            bail!("the event and the challenge need a name (letters or digits)");
        }
        let dir = root.join(&e).join(&n);
        if dir.join("challenge.yaml").is_file() {
            return Self::open(root, &format!("{e}/{n}"));
        }
        for d in ["files", "logs"] {
            std::fs::create_dir_all(dir.join(d))
                .with_context(|| format!("creating {}", dir.join(d).display()))?;
        }
        let ws = Workspace {
            dir,
            id: format!("{e}/{n}"),
            info: Info {
                event: event.trim().to_string(),
                name: name.trim().to_string(),
                target: target.trim().to_string(),
                flag_format: flag_prefix(flag),
                created: crate::runlog::stamp_iso_now(),
                ..Default::default()
            },
        };
        ws.save()?;
        if !ws.notes_path().exists() {
            let notes = format!(
                "# {}\n\nEvent: {}\nTarget: {}\n\n## Notes\n\n",
                ws.info.name,
                ws.info.event,
                if ws.info.target.is_empty() {
                    "-"
                } else {
                    &ws.info.target
                }
            );
            std::fs::write(ws.notes_path(), notes)?;
        }
        Ok(ws)
    }

    /// Open the challenge with id `<event>/<challenge>` under `root`. The id
    /// must name folders directly under the root; nothing else is accepted.
    pub fn open(root: &Path, id: &str) -> Result<Self> {
        let Some((e, n)) = id.split_once('/') else {
            bail!("not a challenge id: {id:?}");
        };
        if e.is_empty() || n.is_empty() || slug(e) != e || slug(n) != n {
            bail!("not a challenge id: {id:?}");
        }
        let dir = root.join(e).join(n);
        let text = std::fs::read_to_string(dir.join("challenge.yaml"))
            .with_context(|| format!("no challenge at {}", dir.display()))?;
        let info: Info = serde_yaml::from_str(&text).context("reading challenge.yaml")?;
        Ok(Workspace {
            dir,
            id: id.to_string(),
            info,
        })
    }

    pub fn save(&self) -> Result<()> {
        let body = format!(
            "# winsploit challenge — edit here or with C in the dashboard\n{}",
            serde_yaml::to_string(&self.info)?
        );
        std::fs::write(self.dir.join("challenge.yaml"), body)?;
        Ok(())
    }

    /// Challenge files, by name.
    pub fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.files_dir())
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_file())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Copy `src` into `files\` (the original is left alone); a name that's
    /// taken gets ` (2)`, ` (3)`, ... Returns the copy's path.
    pub fn add_file(&self, src: &Path) -> Result<PathBuf> {
        if !src.is_file() {
            bail!("{} isn't a file", src.display());
        }
        std::fs::create_dir_all(self.files_dir())?;
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .context("the file has no name")?;
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut dest = self.files_dir().join(&name);
        let mut k = 2;
        while dest.exists() {
            dest = self.files_dir().join(format!("{stem} ({k}){ext}"));
            k += 1;
        }
        std::fs::copy(src, &dest)
            .with_context(|| format!("copying {} into the challenge", src.display()))?;
        Ok(dest)
    }
}

/// Every challenge under `root`, newest first (by challenge.yaml's time).
pub fn list(root: &Path) -> Vec<Workspace> {
    let mut out: Vec<(std::time::SystemTime, Workspace)> = Vec::new();
    let Ok(events) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    for ev in events.flatten().filter(|e| e.path().is_dir()) {
        let Ok(chals) = std::fs::read_dir(ev.path()) else {
            continue;
        };
        for ch in chals.flatten().filter(|e| e.path().is_dir()) {
            let id = format!(
                "{}/{}",
                ev.file_name().to_string_lossy(),
                ch.file_name().to_string_lossy()
            );
            if let Ok(ws) = Workspace::open(root, &id) {
                let t = std::fs::metadata(ws.dir.join("challenge.yaml"))
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                out.push((t, ws));
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    out.into_iter().map(|(_, w)| w).collect()
}

/// Where the active challenge's id is remembered between runs.
fn active_file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("winsploit").join("challenge.txt"))
}

/// Remember (or forget, with None) the active challenge.
pub fn remember(id: Option<&str>) {
    let Some(p) = active_file() else { return };
    match id {
        Some(id) => {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, id);
        }
        None => {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// The challenge that was active last time, if it still exists.
pub fn recall(root: &Path) -> Option<Workspace> {
    let id = std::fs::read_to_string(active_file()?).ok()?;
    Workspace::open(root, id.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join("winsploit-workspace-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn slugs_are_folder_safe() {
        assert_eq!(slug("picoCTF 2026"), "picoCTF-2026");
        assert_eq!(slug("  rev: crackme #1 "), "rev-crackme-1");
        assert_eq!(slug("../../Windows"), "Windows");
        assert_eq!(slug("..."), "");
        assert_eq!(flag_prefix(" ACME{ "), "ACME");
        assert_eq!(flag_prefix("flag{}"), "flag");
    }

    #[test]
    fn create_open_list_and_files() {
        let root = tmp("create");
        let ws = Workspace::create(&root, "picoCTF 2026", "rev crackme1", "10.10.10.3", "ACME{")
            .unwrap();
        assert_eq!(ws.id, "picoCTF-2026/rev-crackme1");
        assert!(ws.files_dir().is_dir() && ws.logs_dir().is_dir());
        assert!(std::fs::read_to_string(ws.notes_path())
            .unwrap()
            .contains("Target: 10.10.10.3"));
        assert_eq!(ws.info.flag_format, "ACME");
        // reopening keeps what's there
        let again = Workspace::create(&root, "picoCTF 2026", "rev crackme1", "", "").unwrap();
        assert_eq!(again.info.target, "10.10.10.3");
        assert_eq!(Workspace::open(&root, &ws.id).unwrap().info, ws.info);
        Workspace::create(&root, "HTB", "Lame", "", "").unwrap();
        let ids: Vec<String> = list(&root).into_iter().map(|w| w.id).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert!(ids.contains(&"HTB/Lame".to_string()));

        // files are copied in, never moved; clashes get a number
        let src = root.join("chall.bin");
        std::fs::write(&src, b"MZ").unwrap();
        let a = ws.add_file(&src).unwrap();
        let b = ws.add_file(&src).unwrap();
        assert!(src.exists());
        assert_eq!(a.file_name().unwrap(), "chall.bin");
        assert_eq!(b.file_name().unwrap(), "chall (2).bin");
        assert_eq!(ws.files(), ["chall (2).bin", "chall.bin"]);
        assert!(ws.add_file(&root).is_err(), "folders aren't files");
    }

    #[test]
    fn ids_never_escape_the_root() {
        let root = tmp("ids");
        Workspace::create(&root, "e", "c", "", "").unwrap();
        for bad in [
            "..",
            "e",
            "e/../..",
            "../e/c",
            "e/c/x",
            "/e/c",
            "e/",
            "C:/Windows",
        ] {
            assert!(Workspace::open(&root, bad).is_err(), "{bad} was accepted");
        }
        assert!(Workspace::open(&root, "e/c").is_ok());
    }
}
