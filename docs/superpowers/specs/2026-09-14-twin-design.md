# Twin — design spec

Date: 2026-09-14
Status: draft for review

## 1. What Twin is

Twin keeps two personal workstations, a macOS laptop and an Omarchy (Arch)
desktop, in sync with an installer-style app: Connect, Diagnose, Sync.
It looks native on each OS (SwiftUI on macOS, a themed terminal UI on
Omarchy) and shares one Rust core that does all the real work.

Goals

- One button to connect the two machines on the same LAN, no account.
- One button to diagnose both machines and fix what is missing.
- One button to sync everything the user selected, with per-item progress.
- Icon-first UI. Every syncable thing is a card with an icon, a count, a
  size, and a location, and can be selected individually or all at once.
- Publishable on GitHub and installable like any other Mac or Omarchy app.

Non-goals (v1)

- Live sharing of a running terminal session across machines. Twin syncs
  tmux layouts and gives a one-command attach; it does not mirror a live
  pty.
- Three or more machines. The model is one pair.
- Syncing Claude credentials, trust state, or plugin caches.
- Merging git repos that diverged on both machines. Twin refuses and
  shows the user, git resolves.

## 2. Architecture

```
twin/
  core/        Rust workspace: twin-core (library) + twin (CLI binary)
  mac/         SwiftUI app "Twin.app", embeds the twin binary
  linux/       Rust ratatui app "twin-tui", links twin-core directly
  install.sh   curl | bash installer for Omarchy
  .github/     CI: build core on both OSes, TUI on Linux, app on macOS, release on tag
```

### 2.1 Core (`twin-core`, `twin`)

One Rust crate holds every operation. The CLI exposes each as a
subcommand and streams NDJSON events on stdout so any front end can drive
it. The SwiftUI app spawns the CLI. The Linux TUI links the library and
receives the same events through a channel.

Subcommands

| Command | What it does |
|---|---|
| `twin discover` | Browse `_twin._tcp` on the LAN, emit peers |
| `twin pair` | Run the pairing handshake with a discovered peer |
| `twin diagnose` | Run every check locally and on the peer over SSH |
| `twin fix <check>` | Install or configure one failed check |
| `twin inventory` | Emit the syncable items with counts, sizes, paths |
| `twin sync [items…] [--all]` | Run the selected sync engines |
| `twin attach` | Open a persistent terminal on the peer (see 5.5) |
| `twin daemon` | Advertise on mDNS and answer pairing and inventory requests |
| `twin schedule on|off` | Install or remove the background sync timer |

Event stream (one JSON object per line)

```
{"ev":"step","id":"claude","state":"running","msg":"comparing 1,204 files"}
{"ev":"progress","id":"claude","done":812,"total":1204,"bytes":533000000}
{"ev":"step","id":"claude","state":"ok","msg":"42 sessions to Linux, 3 to Mac"}
{"ev":"conflict","id":"folders","path":"Sanada/notes.md","kept":"both"}
{"ev":"done","ok":true}
```

State lives in `~/.twin/`:

```
~/.twin/
  config.toml       peer name, peer ssh alias, selections, schedule
  identity/         this machine's twin ssh key pair
  log/              one log per run
  state/            last-sync timestamps per item, git-sync enrolments
```

### 2.2 macOS app (`mac/`)

SwiftUI, macOS 15+. Standard installer window: fixed size, sidebar of
six steps on the left with SF Symbols, content on the right, Back and
Continue at the bottom. System font, system materials, SF Symbols
throughout, `ProgressView` and symbol effects for loading. No custom
colors beyond the accent color. The app bundle embeds the `twin` binary
and calls it with `Process`, parsing the NDJSON stream into
`@Observable` state.

Distribution: notarized DMG on GitHub Releases plus a Homebrew cask tap.

### 2.3 Omarchy app (`linux/`)

ratatui + crossterm, launched in a floating terminal window. It uses only
the 16 ANSI colors and the terminal's default foreground and background,
so it inherits whatever Omarchy theme is active with no parsing. Icons
are Nerd Font glyphs, which Omarchy ships by default. Spinners use
braille frames. Same six steps, same layout: step list on the left,
content on the right, key hints at the bottom, mouse enabled.

Install: `install.sh` downloads the release binary to `~/.local/bin`,
writes a `.desktop` entry with `Terminal=true`, and adds a Hyprland
`windowrule = float, class:^(twin)$` so it opens floating like Omarchy's
own tools. Later: an AUR package.

## 3. The six screens

1. Welcome. Icon, one sentence, Continue.
2. Connect. One button, "Find the other machine." Twin browses mDNS,
   lists peers, and on selection shows the same 6-digit code on both
   screens. Confirming on both exchanges SSH public keys and writes an
   SSH config alias `twin-peer` on each side. A secondary "Away from home"
   link explains Tailscale and enables it if both sides have it.
3. Diagnose. One button. A grid of check cards, each an icon with a
   spinner, then green, amber, or red. Amber and red cards show a Fix
   button. Continue is enabled once nothing is red.
4. Choose. Grid of item cards (section 5). Each card shows icon, name,
   count, size, and location, expands to show its members with
   checkboxes, and has Select All at the top of the screen.
5. Sync. One button. Each selected card animates while running and shows
   what moved each way when done. Conflicts are listed with the path and
   what Twin kept.
6. Done. Summary, a "Keep in sync every 15 minutes" toggle that installs
   the launchd agent or systemd user timer, and a "Sync again" button.

## 4. Connect and diagnose

### 4.1 Pairing

- `twin daemon` runs on both machines (launchd agent on macOS, systemd
  user service on Linux) and advertises `_twin._tcp` with the hostname,
  OS, and a random per-boot instance id.
- Pairing: the initiator connects to the peer daemon over TCP, both
  sides derive a 6-digit code from a SPAKE2-style exchange and show it.
  On confirmation each side sends its `~/.twin/identity` public key; the
  receiver appends it to `~/.ssh/authorized_keys` restricted to the twin
  key, and writes `Host twin-peer` into `~/.ssh/config` pointing at the
  peer's LAN address with the twin identity file. Every later operation
  is plain SSH.
- macOS Remote Login (sshd) is enabled by Twin with a one-time admin
  prompt if it is off. Omarchy runs sshd via `systemctl enable --now sshd`.
- Away from home: if `tailscale status` succeeds on both, Twin rewrites
  the `twin-peer` HostName to the peer's MagicDNS name.

### 4.2 Diagnose checks

Run on both machines, results merged into one grid.

| Check | Pass condition | Fix |
|---|---|---|
| ssh | `ssh twin-peer true` works both ways | re-pair |
| rsync | rsync 3.x present | brew / pacman install |
| git | git present, `user.name` set | install, prompt |
| tmux | tmux present, `~/.tmux.conf` exists | install, seed config |
| tmux-resurrect | plugin present | install via tpm |
| eternal terminal | `et` on mac, `etserver` active on linux | install, enable unit |
| atuin | atuin present, shell hook installed | install, add hook |
| atuin server | `atuin-server` active on linux, reachable from mac | install, enable unit |
| chezmoi | present, source dir initialized | install, init |
| claude | `claude` present, `~/.claude/projects` readable | show install link |
| disk | free space on both > 2x pending transfer | show |
| layout | every selected repo path exists at the same home-relative path on the peer | offer to clone |
| conflicts | no `.twin-conflict-*` files left from earlier runs | list |

## 5. Syncable items and their engines

Every item has the same contract: `inventory()` returns members with
size and path, `sync(selection)` runs bidirectionally, emits events, and
never deletes user data without leaving a conflict copy.

### 5.1 Claude sessions

- Source: `~/.claude/projects/<slug>/` on each side.
- Slug translation: Twin maps `-Users-asim-…` to `-home-asim-…` (and back)
  by replacing the home-prefix slug. Nothing else in `~/.claude` is
  touched. No env vars, no wrapper, no re-login. Verified 2026-09-14 that
  the `CLAUDE_CODE_PROJECT_DIR_NAME` route breaks credentials, so it is
  rejected.
- Merge rules per file type:
  - `*.jsonl` transcripts: append-only, so the longer file wins. Identical
    prefix is asserted before overwriting; a mismatch produces a conflict
    copy instead.
  - `memory/*.md`: newest mtime wins, the loser is kept as
    `name.twin-conflict-<host>-<time>.md` in the same folder.
  - Everything else: newest wins.
- Inventory shows per project: session count, size, path on each machine.
- Guard: before syncing a project Twin checks for a Claude process whose
  cwd is that project on either machine and warns.

### 5.2 Git repos

- Inventory: every `.git` found under `~` to depth 4, excluding
  `node_modules`, caches, and `~/Library`. Shows name, path, size, branch,
  remote or "no remote", dirty state.
- Engine: the git-sync algorithm (Simon Thum's, reimplemented in Rust):
  commit dirty work on the current branch with a `twin: <host> <time>`
  message, fetch, then push if ahead, fast-forward if behind, rebase then
  push if diverged, refuse and report on rebase conflict or on any state
  it cannot prove safe (detached HEAD, in-progress merge or rebase).
- Auto-commit is opt-in per branch, recorded in `~/.twin/state`. The
  Choose screen asks once per repo. Unchecked repos are still fetched and
  fast-forwarded; they are never committed to.
- Remotes: repos with a GitHub remote use it. Repos without one get a bare
  repo on the desktop at `~/git/<name>.git`, and the Mac clone's `origin`
  is set to `twin-peer:git/<name>.git`. The desktop working copy also
  points at the bare repo. Twin creates the bare repo and the peer clone
  on first sync.
- Layout mirror: each repo lives at the same home-relative path on both
  machines. Missing on one side means clone, never move.

### 5.3 Dotfiles

- chezmoi is the engine. Twin initializes the chezmoi source dir as a git
  repo and syncs that repo peer to peer with the same git-sync algorithm,
  pushing into the peer's source dir with `receive.denyCurrentBranch=updateInstead`.
  After a successful sync Twin runs `chezmoi apply` on both sides.
- Seeded files: `.zshrc`, `.zprofile`, `.tmux.conf`, `.gitconfig`,
  `.config/atuin/config.toml`, `.config/ghostty/config`,
  `.claude/settings.json`, `.claude/CLAUDE.md`. Mac-only files
  (`.config/karabiner`, `.config/aerospace`) are templated to apply on
  darwin only.
- Inventory shows the file list with the OS badge for OS-specific ones.

### 5.4 Shell history

- atuin on both. `atuin-server` runs on the desktop with the SQLite
  backend as a systemd user unit, bound to `0.0.0.0:8888` but firewalled
  to the LAN and Tailscale interfaces. No third-party account.
- Twin registers the account on first sync and copies the encryption key
  to the peer over SSH. Sync item just runs `atuin sync` on both sides;
  when the desktop is off, the laptop queues and catches up later.
- Twin runs `atuin import auto` once per machine so existing history is
  kept.

### 5.5 Terminal

- tmux on both, tmux-resurrect saving layouts. Twin syncs the resurrect
  save dir with path translation applied to working directories.
- `twin attach` from either machine runs Eternal Terminal to the peer and
  attaches to (or creates) the `main` tmux session, so a session survives
  sleep and network changes. The Done screen shows the alias.
- Inventory shows the number of saved sessions and windows on each side.

### 5.6 Folders

- Non-repo folders the user adds. Seeded with `~/Sanada` and
  `~/NabdaTech`. rsync both ways, newest wins, loser kept as a
  `.twin-conflict-*` copy. Default ignore list: `node_modules`, `.venv`,
  `target`, `dist`, `.next`, `__pycache__`, `.DS_Store`.
- Never allowed to contain a `.git` directory. If one is found, the folder
  is shown under Git instead.

## 6. Background mode

`twin schedule on` installs a launchd agent on macOS and a systemd user
timer on Linux that runs `twin sync --all --quiet` every 15 minutes and
on sleep (sleepwatcher on macOS, `systemd-sleep` hook on Linux). The
selection is the one saved on the Choose screen. Runs are skipped if the
peer is unreachable and logged to `~/.twin/log`.

## 7. Safety rules

- Twin never deletes a user file. Losers of a merge become conflict
  copies next to the winner.
- Twin never force-pushes and never rewrites history.
- Twin never touches `~/.claude` outside `projects/`.
- Two Twin runs cannot overlap: a lock file in `~/.twin/state` on both
  machines.
- Every run writes a log with the exact rsync and git commands used.

## 8. Testing

- `twin-core`: unit tests per engine using temp directories, including
  the slug translation, the longer-file rule, the conflict-copy rule, and
  each git-sync branch state.
- Integration: a script spins up two local "machines" as two home dirs
  with SSH to localhost and runs a full connect, diagnose, and sync.
- UI: SwiftUI previews per screen; the TUI has snapshot tests of each
  screen at 100x30.
- Manual acceptance on the real pair: attach from each side, resume a
  Claude session started on the other machine, git round trip, atuin
  round trip.

## 9. Phases

1. Core: discover, pair, diagnose, inventory, sync for Claude and Git,
   usable from the terminal on both machines.
2. macOS app over the core, all six screens.
3. Omarchy TUI over the core, all six screens, install.sh.
4. Dotfiles, history, terminal, folders engines. Background mode. CI and
   release packaging.

## 10. Open assumptions

- The GitHub repos under 1-800-CASUALME are solo work, so auto-commit on
  the current branch is acceptable once opted in.
- Both machines run the same major Claude Code version so transcript
  formats match.
- The desktop stays on most of the time; when it is off, Twin simply
  reports "peer unreachable" and does nothing.
