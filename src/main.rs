//! nasrya — native-Windows security toolbox & command runner.
//!
//! Usage:
//!   nasrya [--catalog <path>]         launch the interactive dashboard (TUI)
//!   nasrya [--catalog <path>] list    print the tool catalog and exit
//!   nasrya --help                     show this help
//!   nasrya --version                  show version

mod app;
mod args;
mod banner;
mod detect;
mod registry;
mod runner;
mod scoop;
mod ui;

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // --catalog <path> / --catalog=<path> may appear anywhere.
    let mut catalog: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if let Some(v) = args[i].strip_prefix("--catalog=") {
            catalog = Some(PathBuf::from(v));
            args.remove(i);
        } else if args[i] == "--catalog" {
            args.remove(i);
            if i >= args.len() {
                eprintln!("nasrya: --catalog needs a path");
                std::process::exit(2);
            }
            catalog = Some(PathBuf::from(args.remove(i)));
        } else {
            i += 1;
        }
    }

    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") | Some("help") => {
            print_help();
            Ok(())
        }
        Some("--version") | Some("-V") => {
            println!("nasrya {VERSION}");
            Ok(())
        }
        Some("list") => list(catalog.as_deref()),
        Some(other) => {
            eprintln!("nasrya: unknown command '{other}'. Try `nasrya --help`.");
            std::process::exit(2);
        }
        None => run_tui(catalog.as_deref()),
    }
}

/// Locate and load the catalog, warning (on stderr) about an ignored
/// `.\tools.yaml`.
fn load_catalog(explicit: Option<&Path>) -> Result<(PathBuf, registry::Catalog)> {
    let path = registry::find_catalog(explicit)?;
    if let Some(ignored) = registry::ignored_cwd_catalog(&path) {
        eprintln!(
            "nasrya: note: ignoring {} (catalogs are never loaded from the current directory \
             implicitly; use --catalog to opt in)",
            ignored.display()
        );
    }
    let cat = registry::load(&path)?;
    Ok((path, cat))
}

fn list(explicit: Option<&Path>) -> Result<()> {
    let (path, cat) = load_catalog(explicit)?;
    let tools = cat.tools;
    let name_w = tools
        .iter()
        .map(|t| t.name.chars().count())
        .max()
        .unwrap_or(0);
    let cat_w = tools
        .iter()
        .map(|t| t.category.chars().count())
        .max()
        .unwrap_or(0);

    // Write through a locked handle and stop quietly if the reader goes away
    // (`nasrya list | Select-Object -First 5`), instead of panicking like
    // println! does on a closed pipe.
    let mut out = std::io::stdout().lock();
    let res = (|| -> std::io::Result<()> {
        writeln!(out, "nasrya — {} tools  ({})", tools.len(), path.display())?;
        for t in &tools {
            let installed = if detect::is_installed(&t.exe) {
                "installed"
            } else {
                "missing"
            };
            writeln!(
                out,
                "  {:<name_w$} [{:<cat_w$}] {:<9} {}",
                t.name, t.category, installed, t.description
            )?;
        }
        out.flush()
    })();
    match res {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        // Windows reports a closed pipe as ERROR_NO_DATA (232)
        Err(e) if e.raw_os_error() == Some(232) => Ok(()),
        other => other.context("writing tool list"),
    }
}

fn run_tui(explicit: Option<&Path>) -> Result<()> {
    let (path, catalog) = load_catalog(explicit)?;
    if catalog.tools.is_empty() {
        eprintln!("nasrya: no tools found in {}", path.display());
        return Ok(());
    }
    let mut terminal = ratatui::init();
    let result = App::new(catalog, path).run(&mut terminal);
    ratatui::restore();
    result
}

fn print_help() {
    println!(
        "{}\n\
nasrya {VERSION} — native-Windows security toolbox & command runner\n\n\
USAGE:\n\
    nasrya [--catalog <path>]         launch the interactive dashboard\n\
    nasrya [--catalog <path>] list    list the tool catalog with install status\n\
    nasrya --help                     show this help\n\
    nasrya --version                  show version\n\n\
CATALOG (tools.yaml) is looked up in this order:\n\
    --catalog <path>, then %NASRYA_CATALOG%, then next to nasrya.exe,\n\
    then the source checkout (dev builds), then %APPDATA%\\nasrya\\tools.yaml.\n\
    It is never read from the current directory unless you pass --catalog.",
        banner::LOGO
    );
}
