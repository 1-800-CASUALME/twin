#!/usr/bin/env bash
# Twin installer for Omarchy / Arch Linux.
#   curl -fsSL https://raw.githubusercontent.com/1-800-CASUALME/twin/main/install.sh | bash
# Installs the twin CLI and twin-tui into ~/.local/bin, registers a floating
# app entry in the Omarchy launcher, and enables sshd so the Mac can reach in.
set -euo pipefail

REPO="1-800-CASUALME/twin"
BIN="$HOME/.local/bin"
DRY="${TWIN_DRY_RUN:-}"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
run()  { if [ -n "$DRY" ]; then echo "  $ $*"; else "$@"; fi; }

say "Twin installer"
if ! command -v pacman >/dev/null 2>&1; then
  echo "This installer targets Omarchy / Arch Linux (pacman not found)." >&2
  [ -n "$DRY" ] || exit 1
fi

say "Installing prerequisites (git, rsync, tmux, openssh)"
run sudo pacman -S --needed --noconfirm git rsync tmux openssh

mkdir -p "$BIN"
ARCH="$(uname -m)"
TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1 || true)"

if [ -n "$TAG" ]; then
  say "Downloading Twin $TAG for linux-$ARCH"
  URL="https://github.com/$REPO/releases/download/$TAG/twin-linux-$ARCH.tar.gz"
  run bash -c "curl -fsSL '$URL' | tar -xz -C '$BIN' twin twin-tui"
else
  say "No release found, building from source (needs cargo)"
  if ! command -v cargo >/dev/null 2>&1; then
    say "Installing rust via pacman"
    run sudo pacman -S --needed --noconfirm rust
  fi
  SRC="${TMPDIR:-/tmp}/twin-src"
  run rm -rf "$SRC"
  run git clone -q --depth 1 "https://github.com/$REPO.git" "$SRC"
  run bash -c "cd '$SRC/core' && cargo build --release -q -p twin && install -m755 target/release/twin '$BIN/twin'"
  run bash -c "cd '$SRC/linux' && cargo build --release -q && install -m755 target/release/twin-tui '$BIN/twin-tui'"
fi

say "Registering the app entry"
APPS="$HOME/.local/share/applications"
mkdir -p "$APPS"
TERM_CMD="alacritty --class twin -e"
command -v alacritty >/dev/null 2>&1 || TERM_CMD="ghostty --class=twin -e"
if [ -n "$DRY" ]; then echo "  write $APPS/twin.desktop"; else
cat > "$APPS/twin.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Twin
Comment=Keep two workstations in sync
Exec=$TERM_CMD $BIN/twin-tui
Icon=utilities-terminal
Terminal=false
Categories=Utility;
DESK
fi

say "Adding the Hyprland float rule"
HYPR="$HOME/.config/hypr"
RULES="$HYPR/windowrules.conf"
[ -f "$RULES" ] || RULES="$HYPR/hyprland.conf"
RULE='windowrule = float, class:^(twin)$'
SIZE='windowrule = size 1100 640, class:^(twin)$'
CENTER='windowrule = center, class:^(twin)$'
if [ -f "$RULES" ] && ! grep -qF "$RULE" "$RULES"; then
  if [ -n "$DRY" ]; then echo "  append rules to $RULES"; else
    printf '\n# Twin\n%s\n%s\n%s\n' "$RULE" "$SIZE" "$CENTER" >> "$RULES"
    command -v hyprctl >/dev/null 2>&1 && hyprctl reload >/dev/null 2>&1 || true
  fi
fi

say "Enabling sshd so the other machine can connect"
run sudo systemctl enable --now sshd

case ":$PATH:" in *":$BIN:"*) ;; *) say "Note: add $BIN to your PATH (Omarchy does this by default)";; esac

say "Done. Launch Twin from the app launcher, or run: twin-tui"
