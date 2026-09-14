# Twin

Keep two workstations in sync. One button to connect, one to diagnose, one to sync.

Twin pairs a Mac laptop with a Linux desktop over the LAN and keeps your Claude Code
sessions, git repos, dotfiles, shell history, tmux layouts, and chosen folders in sync,
in both directions. It looks native on each side: SwiftUI on macOS, a themed terminal UI
on Omarchy.

## Install

macOS (planned, Phase 4):

```
brew install --cask 1-800-casualme/twin/twin
```

Omarchy / Arch (planned, Phase 4):

```
curl -fsSL https://raw.githubusercontent.com/1-800-CASUALME/twin/main/install.sh | bash
```

From source (works now):

```
cd core && cargo install --path twin
```

## Use it from the terminal

On the other machine:

```
twin daemon
```

On this one:

```
twin pair          # shows a 6-digit code on both screens, confirm on both
twin diagnose      # checks both machines
twin inventory     # what can be synced, with sizes
twin sync --all    # or: twin sync claude git
```

Every command prints one JSON object per line, so the apps and any script can drive it.

## What syncs and how

| Item | Engine | Rule |
|---|---|---|
| Claude sessions | rsync over SSH | project folders mapped between home paths; transcripts: longer file wins; memory: newer wins, loser kept as `.twin-conflict-*` |
| Git repos | git-sync algorithm | commit (opt-in), fetch, push / fast-forward / rebase; refuses anything unsafe; repos without a remote get a bare repo on the desktop |
| Dotfiles | chezmoi | Phase 4 |
| Shell history | atuin, self-hosted on the desktop | Phase 4 |
| Terminal | tmux-resurrect layouts + `twin attach` over Eternal Terminal | Phase 4 |
| Folders | rsync, newest wins, conflict copies | Phase 4 |

Twin never deletes a user file, never force-pushes, and never touches `~/.claude` outside `projects/`.

## Layout

```
core/     Rust: twin-core library + twin CLI
mac/      SwiftUI app (Phase 2)
linux/    Omarchy terminal UI (Phase 3)
docs/     design spec and implementation plans
```

## Develop

```
cd core
cargo test --workspace     # unit tests
tests/two-homes.sh         # integration test with two fake homes on one machine
```
