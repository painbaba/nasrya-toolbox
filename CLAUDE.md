# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Winsploit (formerly nasrya; the GitHub repo is still `tahabenissa/nasrya-toolbox`) is a native-Windows
ratatui TUI that detects, installs (via Scoop / pipx) and runs pentest tools listed in `tools.yaml`.
Windows-only: detection, installs, process trees and paths all assume Windows.

## Commands

`cargo` lives in `%USERPROFILE%\.cargo\bin`, which may not be on the Bash tool's PATH (use `~/.cargo/bin/cargo`).

```powershell
cargo build                                   # debug build -> target\debug\winsploit.exe
cargo run                                     # dashboard (needs a real interactive terminal)
cargo run -- list                             # non-interactive: catalog + install status
cargo run -- hashid <hash>                    # non-interactive: built-in hash identifier
cargo run -- --catalog .\tools.yaml           # use a specific catalog
cargo test                                    # all unit tests
cargo test ui::tests::categories_get_their_own_row   # one test (substring of the path works too)
cargo fmt --all
cargo clippy --all-targets -- -D warnings     # kept clean, though CI only runs clippy/fmt as advisory
```

CI (`.github/workflows/ci.yml`, windows-latest) hard-gates only on `cargo build --release` plus a smoke
test of `--version` and `list`. Pushing a `v*` tag runs `release.yml`.

A running `winsploit.exe` locks `target\debug\winsploit.exe`, so `cargo build` fails with
"failed to remove file" until that instance is closed.

## Architecture

**Single-threaded event loop with background workers.** `App::run` (app.rs) redraws, drains an
`mpsc` channel of `runner::AppEvent`s, then polls the keyboard for 80 ms. Everything slow runs on
other threads and reports back through that channel:
- `AppEvent::Output { sid, line }` / `Exited { sid, code }` from spawned processes (runner.rs)
- `AppEvent::Detected(..)` from the off-thread detection pass (detect.rs)

Events are routed by **session id**, not by slot. A `Session` (session.rs) is one terminal: output
buffer, stdin pipe, scroll/search state, run log. `App` holds the always-open `console` (id
`CONSOLE` = 0, which takes installs, hash ids and messages) plus `slots: [Option<Session>; SLOTS]` (4
tool terminals). Ids are never reused, so late output from a replaced terminal is simply dropped.
`App::focus` is a *position* (0 = console, 1..=4 = slot).

**Rendering is a pure function of `App`.** `ui::render` reads state and never mutates it, except the
`Cell` in `Session::view` that it fills so scroll math knows the pane size. Input `Mode`s, the
overlays (`appearance`, `show_help`, `tour`) and `confirm` (second press for destructive keys) all
live in `App`, and `on_key` dispatches by whichever is active (tour → appearance → help → mode).

**All styling goes through `theme.rs`.** `Settings` (saved to `%APPDATA%\winsploit\settings.yaml`)
resolves to a `Theme`: a semantic `Palette` (accent/ok/err/warn/info/dim/…), a `Glyphs` set (Unicode
or ASCII) and a border set. Don't hardcode colors or glyphs in ui.rs. Two invariants:
- When `NO_COLOR` is set, crossterm strips *all* colors including backgrounds (`Theme::mono` /
  `App::mono`). Any state shown only by a background color disappears, so use `Theme::selected()` /
  `pill()`, which fall back to reverse video.
- ASCII glyph mode must not emit box-drawing chars or emoji (a test checks this). Gate fancy glyphs
  on `th.glyph.icons` or put them in `Glyphs`.

**The catalog (`tools.yaml`) is the extension point.** Adding a tool needs no code. registry.rs
parses it, folds the legacy `category:` into `categories:` (primary first), expands placeholders
(`{scoop_app}`, `{app:NAME}`), and reports every validation problem at once. `launch:` (stream / gui
/ web / dataset) decides how Enter runs a tool. `verify:` marks a same-named impostor on PATH as a
`Conflict`, and conflicts are never launched. Tools unpackaged elsewhere get a hash-pinned Scoop
manifest in `bucket/`, and `bucket_tests.rs` validates those manifests against the catalog.

**Security invariants (see README "Security model"); keep them intact:**
- `registry::find_catalog` never loads `tools.yaml` from the current directory implicitly. Order:
  `--catalog`, `%WINSPLOIT_CATALOG%`, next to the exe, the source checkout (dev builds), then
  `%APPDATA%\winsploit\tools.yaml`.
- Arguments are split quote-aware into a real argv (args.rs). Never build a shell command string.
- Executables resolve via PATH × PATHEXT, never the cwd. `.cmd`/`.bat` shims go through Rust's
  escaped cmd.exe path.
- Each run and install writes a timestamped log (runlog.rs, under `%LOCALAPPDATA%\winsploit\logs`).

## Testing conventions

- UI tests render into ratatui's `TestBackend` via the `draw(&app, w, h)` / `row(..)` / `screen(..)`
  helpers in `ui.rs` tests, and build `App` from the real `tools.yaml` (`include_str!`). Changes to
  visible strings or layout usually mean updating those assertions.
- Fake sessions in tests keep `pid: None`, so `kill()` never runs `taskkill`.
- Tests must not write the real `%APPDATA%`. `App::finish_tour` skips `settings.save()` under
  `cfg!(test)`; follow that pattern.

## Repo conventions

- Commit and push directly to `main` (no PR branches).
- Commit messages: a short imperative subject, then a wrapped body explaining what changed and why
  (see `git log`).
