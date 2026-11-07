//! The LAN firewall opening. When LAN mode starts, winsploit adds its own
//! narrow inbound rule (TCP, the web view's port, private profile) and
//! removes it again when LAN mode ends, when the app quits, and on the next
//! launch if a crash left it behind. It never touches the firewall as a
//! whole. Rule changes need an elevated shell; without one we say so and
//! print the exact command.
//!
//! Terminal-only on purpose (not an `app::Action`): the web view must never
//! change host firewall state.

use std::path::PathBuf;
use std::process::Command;

/// The rule's name for a port.
pub fn rule_name(port: u16) -> String {
    format!("winsploit-lan-{port}")
}

/// argv for `netsh` (after the program name) that adds the rule.
pub fn add_args(port: u16) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        format!("name={}", rule_name(port)),
        "dir=in".into(),
        "action=allow".into(),
        "protocol=TCP".into(),
        format!("localport={port}"),
        "profile=private".into(),
    ]
}

/// argv for `netsh` that deletes the rule (by exact name only).
pub fn delete_args(port: u16) -> Vec<String> {
    vec![
        "advfirewall".into(),
        "firewall".into(),
        "delete".into(),
        "rule".into(),
        format!("name={}", rule_name(port)),
    ]
}

/// The command to paste into an elevated shell.
pub fn manual_command(args: &[String]) -> String {
    format!("netsh {}", crate::args::join(args))
}

fn netsh(args: &[String]) -> Result<(), String> {
    if cfg!(test) {
        return Err("netsh is not run in tests".into());
    }
    let out = Command::new("netsh")
        .args(args)
        .output()
        .map_err(|e| format!("couldn't run netsh: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Err(if text.is_empty() {
        format!("netsh exited with {}", out.status)
    } else {
        text
    })
}

/// Whether this process is elevated (`fltmc` only works for admins).
pub fn is_elevated() -> bool {
    !cfg!(test)
        && Command::new("fltmc")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn state_file() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("winsploit").join("firewall-rule"))
}

/// Add the rule for `port`. The Err text is for the console and includes
/// the manual command when we aren't elevated.
pub fn open(port: u16) -> Result<(), String> {
    if !is_elevated() {
        return Err(format!(
            "not elevated, so no firewall rule was added (Windows may still ask about \
             winsploit.exe). To open only port {port}, run in an elevated shell: {}",
            manual_command(&add_args(port))
        ));
    }
    netsh(&add_args(port)).map_err(|e| format!("couldn't add the firewall rule: {e}"))?;
    if let Some(f) = state_file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, port.to_string());
    }
    Ok(())
}

/// Remove the rule for `port`.
pub fn close(port: u16) -> Result<(), String> {
    if !is_elevated() {
        return Err(format!(
            "not elevated, so the rule {} was not removed. Run in an elevated shell: {}",
            rule_name(port),
            manual_command(&delete_args(port))
        ));
    }
    let r = netsh(&delete_args(port));
    if let Some(f) = state_file() {
        let _ = std::fs::remove_file(f);
    }
    r.map_err(|e| format!("couldn't remove the firewall rule: {e}"))
}

/// A rule left by a crashed run: its port, if the marker file exists.
pub fn leftover() -> Option<u16> {
    std::fs::read_to_string(state_file()?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_is_named_after_the_port() {
        assert_eq!(rule_name(8080), "winsploit-lan-8080");
    }

    #[test]
    fn add_is_narrow() {
        let a = add_args(4444);
        for want in [
            "name=winsploit-lan-4444",
            "localport=4444",
            "protocol=TCP",
            "profile=private",
            "dir=in",
        ] {
            assert!(a.contains(&want.to_string()), "{want}");
        }
        assert!(!a.iter().any(|x| x == "set" || x.contains("state=off")));
    }

    #[test]
    fn delete_targets_only_our_rule() {
        let d = delete_args(9);
        assert_eq!(d[2], "delete");
        assert_eq!(d.last().unwrap(), "name=winsploit-lan-9");
        assert_eq!(
            manual_command(&d),
            "netsh advfirewall firewall delete rule name=winsploit-lan-9"
        );
    }

    #[test]
    fn tests_never_run_netsh() {
        assert!(!is_elevated());
        assert!(open(1).is_err());
        assert!(leftover().is_none());
    }
}
