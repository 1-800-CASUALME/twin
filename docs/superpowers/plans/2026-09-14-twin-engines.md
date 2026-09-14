# Twin Phase 4: Remaining Engines, Background Sync, Packaging

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Every card in the spec is real: Folders, Dotfiles, History, Terminal join Claude and Git; `twin attach` and `twin schedule` exist; both UIs show the new cards and the schedule toggle; tagged releases publish Linux tarballs, a Mac DMG, and a Homebrew cask.

**Architecture:** The Claude engine's file comparison (list, plan, conflict copies, prefix hashes) moves to `engines/files.rs` and is reused by Folders (newest wins, conflict copies) and Terminal (tmux-resurrect saves, with home-path rewriting). Dotfiles uses chezmoi with a git source dir under `~/.twin/dotfiles` synced peer to peer with the git-sync algorithm. History uses atuin: the hub runs `atuin server` as a systemd user unit on port 8888 with SQLite, and both sides log in with one shared key stored under `~/.twin/state`. Schedule installs a launchd agent or a systemd user timer that runs `twin sync --all` every 15 minutes.

**Spec:** `docs/superpowers/specs/2026-09-14-twin-design.md` §5.3–5.6, §6.

## Tasks

### Task 1: `engines/files.rs` shared module
- Move `FileEntry`, `list_files(root, ignore: &[&str])`, `Plan`, `plan`, `conflict_name`, `prefix_hash`, `conflict_copy`, `write_list` out of `claude.rs`; add `sync_dir(peer, local_dir, remote_dir, ignore, label, emitter) -> Result<Counts>` that runs the full compare-and-transfer for one directory pair (the body of the Claude per-project loop). Claude engine calls `sync_dir` per translated project. Tests move with the code.

### Task 2: Folders engine
- `cfg.folders` (home-relative). Seed on first inventory: `Sanada`, `NabdaTech` if they exist locally. Ignore: `node_modules .venv target dist .next __pycache__ .DS_Store .git`. Refuses a folder containing `.git` at its root (reports it under Git). Inventory member per folder: name, size, file count, presence on each side.

### Task 3: Terminal engine + `twin attach`
- Resurrect dir: first existing of `~/.local/share/tmux/resurrect`, `~/.tmux/resurrect`. Sync it both ways with newest-wins; after pulling, rewrite the peer's home prefix to the local one inside `tmux_resurrect_*.txt` and refresh the `last` symlink. Inventory: number of saves, latest save time.
- `twin attach`: `et twin-peer -c 'tmux new -A -s main'` when `et` exists, else `ssh -t twin-peer tmux new -A -s main`. Uses `exec` semantics (replaces the process).

### Task 4: Dotfiles engine
- Source dir `~/.twin/dotfiles` (git repo). chezmoi config `~/.config/chezmoi/chezmoi.toml` gets `sourceDir` pointing there. First run: `chezmoi add` each seeded file that exists (`.zshrc .zprofile .bashrc .tmux.conf .gitconfig .config/atuin/config.toml .config/ghostty/config .claude/settings.json .claude/CLAUDE.md`), write `.chezmoiignore` with `.config/karabiner` and `.config/aerospace` guarded by `{{ if ne .chezmoi.os "darwin" }}`, commit. Sync: git-sync against `origin` = peer's `~/.twin/dotfiles` (path over `twin-peer:`), both repos set `receive.denyCurrentBranch=updateInstead`; then `chezmoi apply` on both sides. Inventory: managed file list.

### Task 5: History engine (atuin)
- Hub side (`peer.hub == false` means I am hub): write `~/.config/atuin/server.toml` (host 0.0.0.0, port 8888, `db_uri = "sqlite://<home>/.local/share/atuin/server.db"`, open_registration true), install a systemd user unit `atuin-server.service` (`ExecStart=atuin server start`), enable it. Both sides: `~/.config/atuin/config.toml` `sync_address = "http://<hub addr>:8888"`, `auto_sync = true`, `sync_frequency = "5m"`. Account: username = local user, password random, saved to `~/.twin/state/atuin.json` on the hub and copied to the peer; `atuin register` on hub, `atuin key` → `atuin login -u -p -k` on the other side; `atuin import auto` once per side; `atuin sync` both. Every step degrades to a Warn with the exact command to run if a tool is missing.

### Task 6: Schedule + CLI
- `twin schedule on|off|status`. macOS: `~/Library/LaunchAgents/com.asim.twin.sync.plist` with `StartInterval 900`, `ProgramArguments [twin, sync, --all]`, logs to `~/.twin/log/schedule.log`; `launchctl bootstrap gui/<uid>`. Linux: `~/.config/systemd/user/twin-sync.{service,timer}` (`OnUnitActiveSec=15min`), `systemctl --user enable --now twin-sync.timer`. Saves `cfg.schedule`.
- Diagnose additions: `tmux-resurrect` (warn if dir missing), `atuin-server` on hub (warn if unit inactive).

### Task 7: UIs
- Remove placeholder cards; cards come from inventory (now six). Done screen toggle calls `twin schedule on|off` and reflects `cfg.schedule` from `twin status`. TUI: `s` on Done toggles schedule.

### Task 8: Packaging
- `.github/workflows/release.yml` on tag `v*`: build `twin` + `twin-tui` on ubuntu (x86_64) and on an arm64 runner if available → `twin-linux-<arch>.tar.gz`; build `Twin.app` on macos-latest via `mac/build.sh`, zip to `Twin-macos.zip` and `hdiutil` DMG; upload all as release assets. `Casks/twin.rb` points at the DMG. README install lines updated.

### Task 9: Docs, memory, merge.
