//! Scoop integration: works out the command line that installs a tool,
//! adding its bucket first when that bucket isn't on this machine yet.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Buckets Scoop knows by name: `scoop bucket add <name>` needs no URL.
const KNOWN_BUCKETS: &[&str] = &[
    "main",
    "extras",
    "versions",
    "nirsoft",
    "sysinternals",
    "php",
    "nerd-fonts",
    "nonportable",
    "java",
    "games",
];

/// Scoop's install root: `$SCOOP`, else `%USERPROFILE%\scoop`.
pub fn scoop_root() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("SCOOP") {
        return Some(PathBuf::from(s));
    }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("scoop"))
}

/// Is `bucket` already added? (Scoop keeps each bucket as a directory.)
pub fn bucket_added(bucket: &str) -> bool {
    scoop_root()
        .map(|r| r.join("buckets").join(bucket).is_dir())
        .unwrap_or(false)
}

/// Names and URLs end up on a `cmd /C` line, so only allow characters that
/// are inert there.
fn safe(s: &str, allow_url: bool) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '-')
                || (allow_url && matches!(c, ':' | '/' | '?' | '=' | '+' | '~' | '#'))
        })
}

/// `scoop bucket add` line for `bucket`, or `None` if it's already added.
pub fn plan_bucket_add(
    bucket: &str,
    added: bool,
    urls: &BTreeMap<String, String>,
) -> Result<Option<String>, String> {
    if !safe(bucket, false) {
        return Err(format!("refusing unsafe scoop bucket name {bucket:?}"));
    }
    if added {
        return Ok(None);
    }
    match urls.get(bucket) {
        Some(url) if !url.is_empty() => {
            if !safe(url, true) {
                return Err(format!(
                    "refusing unsafe URL for bucket '{bucket}': {url:?}"
                ));
            }
            Ok(Some(format!("scoop bucket add {bucket} {url}")))
        }
        _ if KNOWN_BUCKETS.contains(&bucket) => Ok(Some(format!("scoop bucket add {bucket}"))),
        _ => Err(format!(
            "bucket '{bucket}' isn't added and tools.yaml has no URL for it \
             (add it under `buckets:`, or run `scoop bucket add {bucket} <url>`)"
        )),
    }
}

/// The command(s) that install `pkg`, in order. Pure, so it can be tested:
/// `added` says whether `bucket` is already present, and `urls` holds the
/// catalog's `buckets:` map.
pub fn plan_install(
    pkg: &str,
    bucket: Option<&str>,
    added: bool,
    urls: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    if !safe(pkg, false) {
        return Err(format!("refusing unsafe scoop package name {pkg:?}"));
    }
    let Some(b) = bucket else {
        return Ok(vec![format!("scoop install {pkg}")]);
    };
    if !safe(b, false) {
        return Err(format!("refusing unsafe scoop bucket name {b:?}"));
    }
    let mut steps = Vec::new();
    if !added {
        match urls.get(b) {
            Some(url) if !url.is_empty() => {
                if !safe(url, true) {
                    return Err(format!("refusing unsafe URL for bucket '{b}': {url:?}"));
                }
                steps.push(format!("scoop bucket add {b} {url}"));
            }
            _ if KNOWN_BUCKETS.contains(&b) => steps.push(format!("scoop bucket add {b}")),
            _ => {
                return Err(format!(
                    "bucket '{b}' isn't added and tools.yaml has no URL for it \
                     (add it under `buckets:`, or run `scoop bucket add {b} <url>`)"
                ))
            }
        }
    }
    // bucket/pkg pins the source when several buckets ship the same name
    steps.push(format!("scoop install {b}/{pkg}"));
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls() -> BTreeMap<String, String> {
        BTreeMap::from([(
            "security".to_string(),
            "https://github.com/opsqw/scoop-security".to_string(),
        )])
    }

    #[test]
    fn main_bucket_is_a_plain_install() {
        assert_eq!(
            plan_install("nmap", None, false, &urls()).unwrap(),
            ["scoop install nmap"]
        );
    }

    #[test]
    fn missing_custom_bucket_is_added_from_the_catalog_url() {
        assert_eq!(
            plan_install("nuclei", Some("security"), false, &urls()).unwrap(),
            [
                "scoop bucket add security https://github.com/opsqw/scoop-security",
                "scoop install security/nuclei"
            ]
        );
    }

    #[test]
    fn present_bucket_is_not_re_added() {
        assert_eq!(
            plan_install("nuclei", Some("security"), true, &urls()).unwrap(),
            ["scoop install security/nuclei"]
        );
    }

    #[test]
    fn known_bucket_needs_no_url() {
        assert_eq!(
            plan_install("jadx", Some("extras"), false, &urls()).unwrap(),
            ["scoop bucket add extras", "scoop install extras/jadx"]
        );
    }

    #[test]
    fn unknown_bucket_without_url_is_an_error() {
        assert!(plan_install("x", Some("mystery"), false, &urls()).is_err());
        // ...but fine if it's already added
        assert!(plan_install("x", Some("mystery"), true, &urls()).is_ok());
    }

    #[test]
    fn shell_metacharacters_are_refused() {
        assert!(plan_install("nmap & calc", None, false, &urls()).is_err());
        assert!(plan_install("x", Some("a|b"), true, &urls()).is_err());
        let evil = BTreeMap::from([("s".to_string(), "https://x/&calc".to_string())]);
        assert!(plan_install("x", Some("s"), false, &evil).is_err());
    }
}
