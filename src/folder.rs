//! The working folder: where tools run, changed with `d` like `cd` in a
//! shell. This module turns what was typed into a folder and Tab-completes
//! folder names; app.rs owns the prompt and the current folder.
//!
//! Accepted input, resolved against the current folder:
//! `..` and relative paths, `C:\abs\path`, `D:` (that drive's root), `~` and
//! `~\sub` (the profile), `%VAR%` (environment variables), `-` (the previous
//! folder, handled by the caller) and surrounding quotes from a pasted path.
//! A path to a file means the folder it's in.

use std::path::{Component, Path, PathBuf};

/// Expand `~`, `%VAR%` and quotes in `input` and make it absolute against
/// `cwd`. Doesn't touch the disk.
pub fn expand(cwd: &Path, input: &str) -> PathBuf {
    let s = input.trim().trim_matches('"');
    let s = expand_vars(s);
    let s = if s == "~" {
        home().map(|h| h.display().to_string()).unwrap_or(s)
    } else if let Some(rest) = s.strip_prefix("~\\").or_else(|| s.strip_prefix("~/")) {
        match home() {
            Some(h) => h.join(rest).display().to_string(),
            None => s,
        }
    } else {
        s
    };
    // "D:" alone means that drive's root, not its per-process current folder
    let s = if s.len() == 2 && s.ends_with(':') && s.as_bytes()[0].is_ascii_alphabetic() {
        format!("{s}\\")
    } else {
        s
    };
    normalize(&cwd.join(s))
}

/// The folder `input` names: an existing folder, or the folder an existing
/// file is in. The error says what's wrong, ready for the status line.
pub fn resolve(cwd: &Path, input: &str) -> Result<PathBuf, String> {
    if input.trim().is_empty() {
        return Err("type a folder: a path, .. or ~ (Tab completes)".into());
    }
    let p = expand(cwd, input);
    if p.is_dir() {
        Ok(p)
    } else if p.is_file() {
        p.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("{} has no folder", p.display()))
    } else {
        Err(format!("no such folder: {}", p.display()))
    }
}

/// Folder names that complete `input`: each candidate is the whole input
/// text with its last part completed, ending in `\`, sorted case-insensitively.
/// `input` keeps its own spelling (relative stays relative).
pub fn complete(cwd: &Path, input: &str) -> Vec<String> {
    let text = input.trim_start().trim_start_matches('"');
    // split into the part already typed and the name being completed
    let cut = text.rfind(['\\', '/']).map_or(0, |i| i + 1);
    let (head, partial) = text.split_at(cut);
    // "D:" (no slash yet) completes inside that drive's root
    let (head, partial) = if head.is_empty() && partial.len() == 2 && partial.ends_with(':') {
        (text, "")
    } else {
        (head, partial)
    };
    let dir = if head.is_empty() {
        cwd.to_path_buf()
    } else {
        expand(cwd, head)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let want = partial.to_lowercase();
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.to_lowercase().starts_with(&want))
        // hidden dot-folders only when asked for
        .filter(|n| !n.starts_with('.') || partial.starts_with('.'))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    let sep = if head.is_empty() || head.ends_with(['\\', '/']) {
        ""
    } else {
        "\\"
    };
    names
        .into_iter()
        .map(|n| format!("{head}{sep}{n}\\"))
        .collect()
}

/// `path` for display: the profile shown as `~`, so long CTF paths stay
/// readable in the prompt.
pub fn short(path: &Path) -> String {
    let full = path.display().to_string();
    if let Some(h) = home() {
        let h = h.display().to_string();
        // get(): a non-ASCII path may not split at the profile's byte length
        if let (Some(head), Some(rest)) = (full.get(..h.len()), full.get(h.len()..)) {
            if head.eq_ignore_ascii_case(&h) && (rest.is_empty() || rest.starts_with('\\')) {
                return format!("~{rest}");
            }
        }
    }
    full
}

/// `path` shortened from the left to at most `max` characters, keeping the
/// end (the part that tells folders apart): `…\HackTheBox\Lame`.
pub fn fit(path: &str, max: usize) -> String {
    let n = path.chars().count();
    if n <= max {
        return path.to_string();
    }
    if max <= 1 {
        return "…".chars().take(max).collect();
    }
    let tail: String = path.chars().skip(n - (max - 1)).collect();
    // start at a folder boundary when there is one
    let tail = match tail.find('\\') {
        Some(i) if i > 0 && tail.len() - i > 1 => tail[i..].to_string(),
        _ => tail,
    };
    format!("…{tail}")
}

/// The folder Winsploit starts in: `-C <dir>` when given, else where it was
/// started from.
pub fn initial(arg: Option<&Path>) -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    match arg {
        Some(a) => resolve(&cwd, &a.display().to_string()).map_err(|e| anyhow::anyhow!("-C: {e}")),
        None => Ok(cwd),
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

/// `%NAME%` → the variable's value; unknown names stay as typed.
fn expand_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// Resolve `.` and `..` without touching the disk (and without the `\\?\`
/// prefix canonicalize would add).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // never climb above the root
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("winsploit-folder-{}", std::process::id()));
        for d in ["Box One\\loot", "box-two", "Crypto", ".git"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("Box One\\notes.txt"), "x").unwrap();
        root
    }

    #[test]
    fn resolves_like_cd() {
        let root = tree();
        let one = root.join("Box One");
        assert_eq!(resolve(&root, "Box One").unwrap(), one);
        assert_eq!(resolve(&root, "\"Box One\"").unwrap(), one);
        assert_eq!(resolve(&one, "..").unwrap(), root);
        assert_eq!(
            resolve(&one, "loot\\..\\..\\Crypto").unwrap(),
            root.join("Crypto")
        );
        assert_eq!(
            resolve(&root, &one.join("loot").display().to_string()).unwrap(),
            one.join("loot")
        );
        // a file means its folder
        assert_eq!(resolve(&root, "Box One\\notes.txt").unwrap(), one);
        assert!(resolve(&root, "nope")
            .unwrap_err()
            .contains("no such folder"));
        assert!(resolve(&root, "  ").is_err());
        // .. never climbs above a drive root
        let top = PathBuf::from("C:\\");
        assert_eq!(expand(&top, "..\\.."), top);
        assert_eq!(expand(&root, "D:"), PathBuf::from("D:\\"));
    }

    #[test]
    fn expands_home_and_variables() {
        let root = tree();
        std::env::set_var("WINSPLOIT_TEST_DIR", &root);
        assert_eq!(
            resolve(&root, "%WINSPLOIT_TEST_DIR%\\Crypto").unwrap(),
            root.join("Crypto")
        );
        assert_eq!(
            expand_vars("%NOT_SET_ANYWHERE_X%\\a"),
            "%NOT_SET_ANYWHERE_X%\\a"
        );
        if let Some(h) = home() {
            assert_eq!(expand(&root, "~"), h);
            assert_eq!(expand(&root, "~\\ctf"), h.join("ctf"));
            assert_eq!(short(&h.join("ctf").join("htb")), "~\\ctf\\htb");
        }
    }

    #[test]
    fn fit_keeps_the_end_of_long_paths() {
        assert_eq!(fit("~\\ctf\\htb", 30), "~\\ctf\\htb");
        let long = "C:\\Users\\someone\\Documents\\CTF Files\\HackTheBox\\Lame";
        let f = fit(long, 20);
        assert!(f.chars().count() <= 20, "{f}");
        assert_eq!(f, "…\\HackTheBox\\Lame");
        assert_eq!(fit("abcdef", 1), "…");
        assert_eq!(fit("abcdef", 0), "");
        // non-ASCII never splits a character
        let f = fit("C:\\é\\ça\\où\\là", 6);
        assert!(f.starts_with('…') && f.chars().count() <= 6, "{f}");
    }

    #[test]
    fn tab_completes_folders_only() {
        let root = tree();
        // case-insensitive, folders only, sorted, hidden ones left out
        assert_eq!(complete(&root, "b"), vec!["Box One\\", "box-two\\"]);
        assert_eq!(
            complete(&root, ""),
            vec!["Box One\\", "box-two\\", "Crypto\\"]
        );
        assert_eq!(complete(&root, "."), vec![".git\\"]);
        assert_eq!(complete(&root, "box one\\"), vec!["box one\\loot\\"]);
        assert_eq!(complete(&root, "Box One\\l"), vec!["Box One\\loot\\"]);
        assert!(complete(&root, "zzz").is_empty());
        let abs = format!("{}\\cr", root.display());
        assert_eq!(
            complete(&root, &abs),
            vec![format!("{}\\Crypto\\", root.display())]
        );
    }
}
