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

# 3. Run it (from the repo root, so it finds tools.yaml)
.\target\release\nasrya
```

Some tools live outside Scoop's main bucket. Add these buckets before installing them:

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
nasrya --help       show help
nasrya --version    show version
```

**Dashboard keys**

| Key | Action |
|---|---|
| `↑ / ↓` | move selection |
| `Enter` | run the selected tool (edit the prefilled args, then Enter) |
| `i` | install the selected tool via Scoop |
| `r` | re-detect what's installed |
| `c` | cancel the running process |
| `q` | quit |

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

## Architecture

```
main.rs        CLI entry (TUI / list / help)
registry.rs    loads the YAML tool catalog            ── security core (careful path)
detect.rs      PATH-probe detection (ground truth)     ──
runner.rs      spawn + stream + kill process tree       ──
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
