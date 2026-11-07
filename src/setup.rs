//! `winsploit setup [--buckets] [--pipx]`: the non-interactive part of the
//! Windows installer. It reuses the install planning in `install.rs`, runs
//! each step with its output on stdout and in a run log, and fails (exit
//! code 1) at the first step that fails.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::install::{self, Prereqs};
use crate::registry::Catalog;
use crate::{detect, runlog, scoop};

/// The steps to run, in order. Pure given `pre` and `added`, so it is tested
/// without Scoop on the machine.
pub fn plan(
    catalog: &Catalog,
    buckets: bool,
    pipx: bool,
    pre: Prereqs,
    added: impl Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    let mut steps = Vec::new();
    if (buckets || pipx) && !pre.scoop {
        return Err(
            "scoop isn't installed (or not on PATH); nothing to set up. \
                    Install it first: irm get.scoop.sh | iex   (see https://scoop.sh)"
                .into(),
        );
    }
    if buckets {
        steps.extend(install::bucket_steps(
            &catalog.tools,
            &catalog.buckets,
            added,
        )?);
    }
    if pipx {
        steps.extend(install::pipx_bootstrap(pre, "setup")?);
    }
    Ok(steps)
}

pub fn run(catalog: &Catalog, buckets: bool, pipx: bool) -> Result<()> {
    if !buckets && !pipx {
        bail!("usage: winsploit setup [--buckets] [--pipx]");
    }
    // The installer may have just put Scoop on the persisted PATH.
    crate::envpath::refresh();
    let have_pipx = detect::resolve("pipx").is_some();
    let pre = Prereqs {
        scoop: detect::resolve("scoop").is_some(),
        pipx: have_pipx,
        python: !have_pipx && python_works(),
        bucket_added: false,
    };
    let steps =
        plan(catalog, buckets, pipx, pre, scoop::bucket_added).map_err(anyhow::Error::msg)?;
    if steps.is_empty() {
        println!("setup: nothing to do (buckets and pipx already in place)");
        return Ok(());
    }

    let command = steps.join(" && ");
    let mut log = runlog::RunLog::create(&runlog::log_dir(), "setup", &command, "setup")
        .context("creating the setup log")?;
    println!("setup: log {}", log.path().display());
    let mut failure = None;
    for step in &steps {
        println!("$ {step}");
        log.line(&format!("$ {step}"));
        match run_step(step, &mut log) {
            Ok(0) => {}
            Ok(code) => {
                failure = Some((step.clone(), code));
                break;
            }
            Err(e) => {
                log.line(&format!("[err] {e}"));
                log.finish(None);
                return Err(e);
            }
        }
    }
    match failure {
        None => {
            log.finish(Some(0));
            println!("setup: done");
            Ok(())
        }
        Some((step, code)) => {
            log.finish(Some(code));
            bail!("setup failed at `{step}` (exit {code})")
        }
    }
}

/// Run one planned step. Scoop and pipx are `.cmd` shims, so it goes through
/// `cmd /D /C`; every step came from the planners, which only emit tokens that
/// are inert on a cmd line.
fn run_step(step: &str, log: &mut runlog::RunLog) -> Result<i32> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    cmd.raw_arg(format!("/D /C {step} 2>&1"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("starting `{step}`"))?;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            println!("{line}");
            log.line(&line);
        }
    }
    Ok(child.wait()?.code().unwrap_or(-1))
}

fn python_works() -> bool {
    detect::resolve("python").is_some()
        && Command::new("python")
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry;

    fn cat() -> Catalog {
        registry::parse(include_str!("../tools.yaml")).unwrap()
    }

    fn pre(scoop: bool) -> Prereqs {
        Prereqs {
            scoop,
            python: false,
            pipx: false,
            bucket_added: false,
        }
    }

    #[test]
    fn plans_buckets_then_pipx() {
        let steps = plan(&cat(), true, true, pre(true), |_| false).unwrap();
        assert!(steps
            .iter()
            .any(|s| s.starts_with("scoop bucket add security ")));
        assert!(steps
            .iter()
            .any(|s| s.starts_with("scoop bucket add winsploit ")));
        let n = steps.len();
        assert_eq!(
            steps[n - 3..],
            [
                "scoop install python",
                "scoop install pipx",
                "pipx ensurepath"
            ]
        );
    }

    #[test]
    fn already_satisfied_steps_are_skipped() {
        let all = Prereqs {
            scoop: true,
            python: true,
            pipx: true,
            bucket_added: true,
        };
        let steps = plan(&cat(), true, false, all, |_| true).unwrap();
        assert!(steps.is_empty());
    }

    #[test]
    fn needs_scoop() {
        let e = plan(&cat(), true, false, pre(false), |_| false).unwrap_err();
        assert!(e.contains("scoop isn't installed"));
    }
}
