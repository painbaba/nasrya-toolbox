//! Tool detection. The PATH probe (`where.exe <exe>`) is the ground truth:
//! it confirms the binary is actually runnable, which is what we care about,
//! rather than trusting a package manager's install database.

use std::process::{Command, Stdio};

pub fn is_installed(exe: &str) -> bool {
    Command::new("where.exe")
        .arg(exe)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
