# Twin

Keep two workstations in sync. One button to connect, one to diagnose, one to sync.

Twin pairs a Mac laptop with a Linux desktop over the LAN and keeps your Claude Code
sessions, git repos, dotfiles, shell history, tmux layouts, and chosen folders in sync,
in both directions. It looks native on each side: SwiftUI on macOS, a themed terminal UI
on Omarchy.

## Install

macOS (recommended, avoids the Gatekeeper warning):

```
curl -fsSL https://raw.githubusercontent.com/1-800-CASUALME/twin/main/install-mac.sh | bash
```

Or with Homebrew:

```
brew tap 1-800-CASUALME/twin https://github.com/1-800-CASUALME/twin
brew install --cask --no-quarantine twin
```

Or download `Twin-macos.dmg` from the latest release and drag Twin onto Applications.
The app is ad-hoc signed, not notarized, so macOS will say it "could not verify" it:
right-click Twin.app, choose Open, and confirm once. The installer script clears that flag for you.
Twin links the `twin` command into `~/.local/bin` on first launch.

Omarchy / Arch:

```
curl -fsSL https://raw.githubusercontent.com/1-800-CASUALME/twin/main/install.sh | bash
```

That installs `twin` and `twin-tui`, adds a floating Twin entry to the launcher, and enables sshd.

From source:

```
cd core  && cargo install --path twin      # the CLI, both machines
cd linux && cargo install --path .         # the Omarchy UI
cd mac   && ./build.sh                     # Twin.app in mac/build/
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
twin sync --all    # or: twin sync claude git dotfiles history terminal folders
twin attach        # a persistent tmux session on the other machine
twin schedule on   # background sync every 15 minutes
```

Every command prints one JSON object per line, so the apps and any script can drive it.

## What syncs and how

| Item | Engine | Rule |
|---|---|---|
| Claude sessions | rsync over SSH | project folders mapped between home paths; transcripts: longer file wins; memory: newer wins, loser kept as `.twin-conflict-*` |
| Git repos | git-sync algorithm | commit (opt-in), fetch, push / fast-forward / rebase; refuses anything unsafe; repos without a remote get a bare repo on the desktop |
| Dotfiles | chezmoi | source repo at `~/.twin/dotfiles` synced peer to peer; Mac-only configs skipped on Linux |
| Shell history | atuin | server self-hosted on the desktop when its atuin build can run one, else the hosted encrypted sync |
| Terminal | tmux-resurrect | saved layouts synced with home paths rewritten; `twin attach` opens the shared `main` session over Eternal Terminal or SSH |
| Folders | rsync | newest wins, loser kept as a `.twin-conflict-*` copy; build dirs ignored |

Twin never deletes a user file, never force-pushes, and never touches `~/.claude` outside `projects/`.

## Layout

```
core/     Rust: twin-core library + twin CLI
mac/      SwiftUI app, built by mac/build.sh into Twin.app
linux/    Omarchy terminal UI (ratatui), inherits the active theme
Casks/    Homebrew cask
docs/     design spec and implementation plans
```

## Release

Tag `vX.Y.Z` and push. CI builds `twin-linux-{x86_64,aarch64}.tar.gz`, `Twin-macos.dmg`,
and `Twin-macos.zip`, and attaches them to the GitHub release. The Mac build is ad-hoc signed, not notarized
(that needs an Apple Developer account); `install-mac.sh` clears the quarantine flag so
Gatekeeper does not block it.

## Develop

```
cd core
cargo test --workspace     # unit tests
tests/two-homes.sh         # integration test with two fake homes on one machine
```
