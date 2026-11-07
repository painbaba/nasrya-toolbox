//! nasrya — native-Windows security toolbox & command runner.
//!
//! Usage:
//!   nasrya            launch the interactive dashboard (TUI)
//!   nasrya list       print the tool catalog and exit
//!   nasrya --help     show this help
//!   nasrya --version  show version

mod app;
mod banner;
mod detect;
mod registry;
mod runner;
mod ui;

use anyhow::Result;
use app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") | Some("help") => {
            print_help();
            Ok(())
        }
        Some("--version") | Some("-V") => {
            println!("nasrya {VERSION}");
            Ok(())
        }
        Some("list") => {
            let tools = registry::load(registry::registry_path())?;
            println!("nasrya — {} tools", tools.len());
            let name_w = tools.iter().map(|t| t.name.chars().count()).max().unwrap_or(0);
            let cat_w = tools.iter().map(|t| t.category.chars().count()).max().unwrap_or(0);
            for t in &tools {
                let installed = if detect::is_installed(&t.exe) {
                    "installed"
                } else {
                    "missing"
                };
                println!(
                    "  {:<name_w$} [{:<cat_w$}] {:<9} {}",
                    t.name, t.category, installed, t.description
                );
            }
            Ok(())
        }
        Some(other) => {
            eprintln!("nasrya: unknown command '{other}'. Try `nasrya --help`.");
            std::process::exit(2);
        }
        None => run_tui(),
    }
}

fn run_tui() -> Result<()> {
    let tools = registry::load(registry::registry_path())?;
    if tools.is_empty() {
        eprintln!("nasrya: no tools found in tools.yaml");
        return Ok(());
    }
    let mut terminal = ratatui::init();
    let result = App::new(tools).run(&mut terminal);
    ratatui::restore();
    result
}

fn print_help() {
    println!(
        "{}\n\
nasrya {VERSION} — native-Windows security toolbox & command runner\n\n\
USAGE:\n\
    nasrya              launch the interactive dashboard\n\
    nasrya list         list the tool catalog with install status\n\
    nasrya --help       show this help\n\
    nasrya --version    show version\n\n\
Tools are declared in tools.yaml — add one by editing that file.",
        banner::LOGO
    );
}
