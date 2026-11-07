<div align="center">

```
 ███╗   ██╗ █████╗ ███████╗██████╗ ██╗   ██╗ █████╗
 ████╗  ██║██╔══██╗██╔════╝██╔══██╗╚██╗ ██╔╝██╔══██╗
 ██╔██╗ ██║███████║███████╗██████╔╝ ╚████╔╝ ███████║
 ██║╚██╗██║██╔══██║╚════██║██╔══██╗  ╚██╔╝  ██╔══██║
 ██║ ╚████║██║  ██║███████║██║  ██║   ██║   ██║  ██║
 ╚═╝  ╚═══╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝
```

**A native-Windows security toolbox & command runner — in your terminal.**

`nasrya` is a fast, beautiful TUI that detects, installs (via [Scoop](https://scoop.sh)) and runs
the pentest tools that actually work natively on Windows — no WSL, no VM.

[![CI](https://github.com/tahabenissa/nasrya-toolbox/actions/workflows/ci.yml/badge.svg)](https://github.com/tahabenissa/nasrya-toolbox/actions/workflows/ci.yml)
![status](https://img.shields.io/badge/status-early%20skeleton-orange)
![platform](https://img.shields.io/badge/platform-Windows-0078D6)
![built with](https://img.shields.io/badge/built%20with-Rust%20%2B%20ratatui-000000)
![license](https://img.shields.io/badge/license-MIT-green)

</div>

---

## ⚠️ Authorized use only

`nasrya` is for **authorized** penetration testing, security research, CTFs, and learning.
Only use it against systems you own or have **explicit written permission** to test.
Unauthorized scanning or attacking of systems is illegal in most jurisdictions. You are
responsible for staying inside your rules of engagement.

---

## Why this exists

Kali Linux is the home of security tooling, but a lot of people work on **Windows**. The existing
options are all compromises:

- **Commando VM / FLARE-VM** — excellent, but one-shot *VM provisioners*, not a day-to-day launcher.
- **PentestBox** — the right idea (native, portable), but abandoned since ~2016 and license-murky.
- **UniGetUI** — a great package GUI, but no security curation and no "run the tool" workflow.

There is **no actively maintained, cleanly-licensed, native-Windows launcher** that both
*installs* and *runs* security tools from one interface. `nasrya` fills that gap — and it's honest
about what genuinely works on Windows versus what needs Linux.

## Features

- 🎛️ **Interactive TUI dashboard** — browse tools by category, see install status at a glance, launch with one key.
- 📦 **Scoop-backed installs** — install a missing tool straight from the dashboard; detection is a real PATH probe, not a guess.
- 📡 **Live output streaming** — a tool's stdout/stderr streams into the output pane in real time, color-coded, and is cancellable (kills the whole process tree).
- 🧩 **Config-driven registry** — every tool is a YAML entry; add one without touching code.
- 🎨 **Looks good** — ASCII-art splash, rounded panels, a cohesive color scheme.

## Honest scope — what works natively on Windows

`nasrya` deliberately curates tools that run *well* on Windows:

| Works great | Works (with notes) | Not on Windows — use Linux |
|---|---|---|
| nmap, nuclei, ffuf, gobuster, httpx, amass | sqlmap, theHarvester, volatility3 (need Python) | **aircrack-ng live capture** (no monitor mode/injection) |
| wireshark / tshark, hashcat, John | hashcat (needs GPU drivers), tshark (needs Npcap + admin) | THC-Hydra, WPScan, WhatWeb, masscan (degraded) |

> **Wireless, honestly:** live Wi-Fi monitor-mode and packet injection **do not work** on Windows
> with normal adapters. Only *offline* cracking of already-captured files is useful here. For real
> wireless work, use Linux with a supported USB adapter.

## Install

**Requirements:** Windows 10/11, [Rust](https://rustup.rs) (to build), and [Scoop](https://scoop.sh) (to install tools from inside the dashboard).

```powershell
# 1. Install Scoop (if you don't have it)
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression

# 2. Build nasrya
git clone https://github.com/tahabenissa/nasrya-toolbox
cd nasrya-toolbox
cargo build --release

# 3. Run it (a dev build finds the checkout's tools.yaml from any directory)
.\target\release\nasrya
```

Some tools live outside Scoop's main bucket. Pressing `i` adds the right bucket automatically
the first time it's needed (URLs for non-standard buckets come from the `buckets:` map in
`tools.yaml`). To add them by hand instead:

```powershell
# nuclei, httpx, subfinder, katana, …
scoop bucket add security https://github.com/opsqw/scoop-security
# Sysinternals (strings), jadx, …
scoop bucket add extras
```

## Usage

```
nasrya              launch the interactive dashboard
nasrya list         list the tool catalog with install status
nasrya hashid <h>   identify a hash (built in: hashcat -m / john --format hints)
nasrya --help       show help
nasrya --version    show version

--catalog <path>    use a specific tools.yaml (works with or without `list`)
```

**Which `tools.yaml` is used?** In order: `--catalog <path>`, then `%NASRYA_CATALOG%`, then the
one next to `nasrya.exe`, then the source checkout (dev builds), then
`%APPDATA%\nasrya\tools.yaml`. The catalog decides what Enter runs, so nasrya **never** picks
up a `tools.yaml` from the current directory on its own. You'll often run it inside untrusted
folders (challenge files, loot, cloned repos). If one is there, nasrya says it was ignored;
pass `--catalog .\tools.yaml` to use it on purpose. The active path is shown in the dashboard.

**Dashboard keys**

| Key | Action |
|---|---|
| `↑ / ↓` | move selection |
| `Enter` | run the selected tool (edit the prefilled args, then Enter). While a tool is running: type a line to send to its input (`Ctrl+D` = end of input) |
| `i` | install the selected tool (Scoop or pipx) |
| `r` | re-detect what's installed |
| `c` | cancel the running process |
| `h` | identify a hash (built in, works offline) |
| `q` | quit |

**Editing the args / hash prompt:** `←/→` move, `Ctrl+←/→` jump words, `Home/End` (or
`Ctrl+A/E`), `Del`, `Ctrl+Backspace` / `Ctrl+W` delete a word, `Ctrl+U` clear to the start,
`↑/↓` cycle through the arguments you used before with that tool (per session). Pasting a
whole command line works too.

**Arguments** are split on spaces. Use `"double"` or `'single'` quotes to keep spaces in
one argument, e.g. `-H "User-Agent: nasrya"` or `-w "C:\My Lists\big.txt"`. Backslashes are
literal (no need to double them), and `\"` is a literal quote. A quoted path can't end in a
backslash: write `"C:\dir"`, not `"C:\dir\"`.

## Adding a tool

Edit [`tools.yaml`](tools.yaml) — no code changes:

```yaml
  - name: gobuster
    category: web
    description: Directory / DNS / vhost brute-forcing
    exe: gobuster          # used to detect installation (PATH probe)
    scoop: gobuster        # scoop package to install
    run_cmd: gobuster      # command actually launched
    args_hint: "dir -u https://example.com -w wordlist.txt"
    bucket: security       # optional: scoop bucket it lives in
```

A bucket that Scoop doesn't know by name also needs its URL in the top-level `buckets:` map,
so nasrya can add it on first install.

**Tools no Scoop bucket packages** (gau, John jumbo, steghide, nikto, ...) get a manifest in
this repo's own Scoop bucket, [`bucket/`](bucket), and `bucket: nasrya` in their entry. Scoop then
handles the download, **SHA-256 verification**, extraction and shims, and nasrya adds the bucket on
first use. A manifest needs a pinned `url` + `hash` (`cargo test` checks every manifest is
well-formed and that every `bucket: nasrya` entry has one).

**Python tools** install with pipx (each in its own venv). If pipx is missing, nasrya installs
Python and pipx through Scoop first:

```yaml
  - name: arjun
    category: web
    description: HTTP parameter discovery
    exe: arjun
    install: pipx
    pipx: arjun            # or git+https://github.com/owner/repo for unreleased tools
    run_cmd: arjun
    args_hint: "-u https://example.com/endpoint"
```

**Desktop and web apps** don't stream output. `launch: gui` starts the app in its own window.
`launch: web` starts `run_cmd` (a local server, output streamed and cancellable with `c`) if
there is one, then opens `url` in your browser:

```yaml
    launch: gui                        # Ghidra, x64dbg, ...
    # or
    launch: web
    url: http://127.0.0.1:5001         # SpiderFoot, ...
```

Some apps aren't put on PATH by Scoop (only a Start-menu shortcut), and some are just an HTML
page. For those, `{scoop_app}` in `exe`, `run_cmd` or `url` expands to the app's Scoop folder:
`exe: '{scoop_app}\dnSpy.exe'`, or `url: '{scoop_app}\CyberChef.html'` with `launch: web`.

**Wordlists and other data** use `launch: dataset`. There's nothing to run, so Enter lists the
files and copies the folder path to the clipboard. `{app:NAME}` points another tool's
arguments at an installed dataset. The shipped `seclists-lite` (the most-used SecLists files,
pinned and hash-checked) is wired into gobuster, ffuf, feroxbuster, dirsearch, wfuzz and john
this way:

```yaml
    args_hint: 'dir -u https://example.com -w "{app:seclists-lite}\common.txt"'
```

Write Windows paths in `'single quotes'`: inside `"double quotes"`, YAML turns `\r`, `\x`, ...
into escape sequences.

Some names are shared by unrelated programs (ProjectDiscovery's `httpx` and the Python
`httpx` CLI, Sysinternals `strings` and GNU `strings`). `verify:` checks that the `exe` found on
PATH is the right one: `verify: { args: "-version", contains: "projectdiscovery" }`. If it
isn't, the tool shows as a conflict (`◐`), along with the path that won, and won't be launched.

`note:` adds a caveat that's shown every time the tool is run, e.g. a Windows limitation.
Bad entries (missing `scoop:`/`pipx:`, a web tool without a `url:`, …) are reported together
when the catalog loads.

## Architecture

```
main.rs        CLI entry (TUI / list / help)
registry.rs    loads + validates the YAML tool catalog ── security core (careful path)
detect.rs      PATH x PATHEXT detection (ground truth) ──
install.rs     install planning (scoop / pipx)         ──
scoop.rs       scoop buckets + package installs        ──
envpath.rs     picks up PATH changes after installs    ──
args.rs        quote-aware argument splitting          ──
hashid.rs      built-in hash identifier                ──
editor.rs      args prompt line editor + history       ──
runner.rs      spawn + stream / detach + kill tree      ──
app.rs         state machine, launch/install control    ──
ui.rs          dashboard rendering                       ── presentation layer
banner.rs      ASCII-art splash + wordmark               ──
```

Config-driven tool registry → detector/installer → process runner (streams live output and tees it)
→ interactive dashboard. Roadmap below extends the security core toward real engagement workflows.

## Roadmap

- [ ] Per-engagement workspaces (scope list, authorization record, audit log) for real pentest workflows
- [ ] Scope enforcement before every launch (block/override-with-reason)
- [ ] Richer tool catalog across all categories (recon / web / passwords / forensics)
- [ ] Output search + scrollback, per-run log files and artifacts
- [ ] One-shot report generation (Markdown/HTML)
- [ ] Scoop bucket auto-add for security tools

## Credits & prior art

Inspired by the native-toolbox idea of **PentestBox**, the provisioning model of
**Commando VM / FLARE-VM**, and the package catalogs of the **Scoop** security buckets. Built with
[ratatui](https://ratatui.rs). Tool curation references
[awesome-pentest](https://github.com/enaqx/awesome-pentest).

## License

MIT — see [LICENSE](LICENSE).
