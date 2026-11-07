//! Tool registry: loads the config-driven catalog from `tools.yaml`.
//! Adding a tool is a YAML edit, never a code change.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    /// Executable name used to detect installation (PATH probe).
    pub exe: String,
    /// Scoop package name used to install.
    pub scoop: String,
    /// Command actually launched.
    pub run_cmd: String,
    #[serde(default)]
    pub args_hint: Option<String>,
    /// Optional scoop bucket the package lives in (main-bucket tools omit this).
    #[serde(default)]
    pub bucket: Option<String>,
}

/// The whole catalog file.
#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub tools: Vec<Tool>,
    /// Extra Scoop buckets: name -> git URL. Buckets Scoop already knows by
    /// name (extras, versions, ...) don't need an entry.
    #[serde(default)]
    pub buckets: BTreeMap<String, String>,
}

/// Where to look for `tools.yaml`, in priority order.
///
/// The catalog decides which commands Enter runs, so it is **never** taken
/// implicitly from the current directory: nasrya is routinely started inside
/// untrusted folders (challenge files, loot, cloned repos), and a planted
/// `tools.yaml` there could point `nmap` at anything. To use one on purpose,
/// pass `--catalog .\tools.yaml` or set `NASRYA_CATALOG`.
pub fn catalog_candidates(explicit: Option<&Path>) -> Vec<PathBuf> {
    if let Some(p) = explicit {
        return vec![p.to_path_buf()];
    }
    if let Some(p) = std::env::var_os("NASRYA_CATALOG") {
        return vec![PathBuf::from(p)];
    }
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        // shipped layout: tools.yaml next to nasrya.exe
        out.push(dir.join("tools.yaml"));
        // dev layout: target\{debug,release}\nasrya.exe inside the nasrya
        // checkout. That's the binary's own source tree, not wherever the
        // user happens to be.
        for root in dir.ancestors().skip(1).take(3) {
            if is_nasrya_checkout(root) {
                out.push(root.join("tools.yaml"));
                break;
            }
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        out.push(PathBuf::from(appdata).join("nasrya").join("tools.yaml"));
    }
    out
}

fn is_nasrya_checkout(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .map(|t| t.lines().any(|l| l.trim() == r#"name = "nasrya""#))
        .unwrap_or(false)
}

/// Resolve the catalog path: the first candidate that exists.
pub fn find_catalog(explicit: Option<&Path>) -> Result<PathBuf> {
    let cands = catalog_candidates(explicit);
    if let Some(p) = cands.iter().find(|p| p.is_file()) {
        return Ok(p.clone());
    }
    let tried: Vec<String> = cands.iter().map(|p| format!("  {}", p.display())).collect();
    anyhow::bail!(
        "no tools.yaml found. Looked in:\n{}\n(use --catalog <path> to point at one)",
        tried.join("\n")
    )
}

/// If there is a `tools.yaml` in the current directory that we are *not*
/// using, return its path so the UI can say it was ignored.
pub fn ignored_cwd_catalog(active: &Path) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?.join("tools.yaml");
    if !cwd.is_file() {
        return None;
    }
    let same = match (fs::canonicalize(&cwd), fs::canonicalize(active)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    (!same).then_some(cwd)
}

pub fn load(path: impl AsRef<Path>) -> Result<Catalog> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading tool registry {}", path.display()))?;
    serde_yaml::from_str(&text).context("parsing tools.yaml")
}
