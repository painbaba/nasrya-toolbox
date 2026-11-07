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

/// Find `tools.yaml`: prefer the current directory, then fall back to the
/// directory containing the executable (so a shipped binary finds its catalog).
pub fn registry_path() -> PathBuf {
    let cwd = Path::new("tools.yaml");
    if cwd.exists() {
        return cwd.to_path_buf();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("tools.yaml");
            if p.exists() {
                return p;
            }
        }
    }
    cwd.to_path_buf()
}

pub fn load(path: impl AsRef<Path>) -> Result<Catalog> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading tool registry {}", path.display()))?;
    serde_yaml::from_str(&text).context("parsing tools.yaml")
}
