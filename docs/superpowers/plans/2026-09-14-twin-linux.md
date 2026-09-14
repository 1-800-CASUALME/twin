# Twin Omarchy TUI (Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `twin-tui`, a ratatui app with the same six steps as the Mac app, that opens in a floating terminal window on Omarchy and inherits the active theme.

**Architecture:** Separate crate in `linux/` depending on `twin-core` by path. The UI thread owns an `App` struct; every core operation runs on a `std::thread` with a `ChannelEmitter`, and the UI drains the channel each tick (50 ms). The pairing daemon runs for the app's lifetime; its accept callback blocks on a reply channel that the UI answers when the user confirms the code. Rendering uses only the 16 ANSI colors and the terminal's default foreground and background. Icons are Nerd Font glyphs with an `--ascii` fallback.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, twin-core.

**Spec:** `docs/superpowers/specs/2026-09-14-twin-design.md` §2.3, §3.

## Global Constraints

- Only ANSI colors: Green for ok, Yellow for warn, Red for fail, Cyan for accent, DarkGray for muted. Never a truecolor value.
- Layout: 22-column step list on the left, content on the right, one-line key hints at the bottom. Works at 100x30 and larger.
- Keys: Enter = primary action or Continue, Tab/j/k/arrows = move focus, Space = toggle, a = select all, b = back, f = fix focused check, q = quit. Mouse clicks on cards toggle them.
- Binary name `twin-tui`; desktop entry `twin.desktop` with `Terminal=true`; Hyprland rule `windowrule = float, class:^(twin)$`; launched as `alacritty --class twin -e twin-tui` (or ghostty if alacritty is missing).

## File structure

```
linux/
  Cargo.toml
  src/main.rs        terminal setup, event loop, key/mouse dispatch
  src/app.rs         App state, actions, background threads
  src/ui.rs          rendering per step
  src/icons.rs       Nerd Font glyphs with ascii fallback
install.sh           at repo root: install twin + twin-tui on Omarchy
```

### Task 1: Crate, App state, background runner
- `App { step, busy, error, peers, code, paired, checks, items, selected, focus, step_states, progress, conflicts, synced, sync_ok, rx: Receiver<Msg>, tx: Sender<Msg>, daemon_reply: Option<Sender<bool>> }`.
- `enum Msg { Ev(Event), AskCode(String, Sender<bool>), Finished(&'static str) }`.
- `App::spawn(label, f: impl FnOnce(&dyn Emitter) + Send)` runs `f` on a thread with a `ChannelEmitter` wrapped so events arrive as `Msg::Ev`, then sends `Finished`.
- Actions: `discover`, `pair(i)`, `answer(bool)`, `diagnose`, `fix`, `inventory`, `toggle`, `select_all`, `sync`, `next`, `back`.
- Verify: `cargo build` in `linux/`.

### Task 2: Rendering
- `ui::draw(f, app)`: outer horizontal split (22 | rest); vertical split of the right side (content | 1-line hints). Sidebar lists steps with `icons::check` for passed, `icons::arrow` for current.
- Welcome: centered logo glyph, title, tagline. Connect: big button line, peer rows, code modal (centered popup). Diagnose: grid of cards 3 per row with icon, name, side, state glyph, message. Choose: cards 2 per row with icon, name, count, size, location, expandable member list (Enter on a card toggles expansion, Space toggles selection). Sync: rows with spinner glyph while running and a Gauge. Done: big check glyph, summary lines, attach hint.
- Verify: run `twin-tui` on macOS in a 100x30 terminal and walk all steps with the CLI daemon paired to a temp home.

### Task 3: install.sh
- Detect Arch (`pacman`), install `git rsync tmux openssh` via pacman `--needed`, install a release binary pair from GitHub if a tag exists else build from source with cargo, put `twin` and `twin-tui` in `~/.local/bin`, write `~/.local/share/applications/twin.desktop`, append the Hyprland float rule to `~/.config/hypr/windowrules.conf` if absent (Omarchy's per-user file), enable `sshd`, and print a summary.
- Verify: shellcheck-clean; dry run on macOS with `TWIN_DRY_RUN=1` prints the steps.

### Task 4: Commit and merge.
