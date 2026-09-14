#!/usr/bin/env bash
# Twin installer for macOS.
#   curl -fsSL https://raw.githubusercontent.com/1-800-CASUALME/twin/main/install-mac.sh | bash
# Downloads the latest release, puts Twin.app in /Applications, clears the quarantine flag
# (the app is ad-hoc signed, not notarized, so Gatekeeper would otherwise refuse it), and
# links the `twin` command into ~/.local/bin.
set -euo pipefail
REPO="1-800-CASUALME/twin"
say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }

TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
[ -n "$TAG" ] || { echo "no release found" >&2; exit 1; }
say "Downloading Twin $TAG"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
curl -fsSL "https://github.com/$REPO/releases/download/$TAG/Twin-macos.zip" -o "$T/Twin.zip"
ditto -x -k "$T/Twin.zip" "$T"
say "Installing to /Applications"
osascript -e 'tell application "Twin" to quit' >/dev/null 2>&1 || true
rm -rf /Applications/Twin.app
ditto "$T/Twin.app" /Applications/Twin.app
xattr -dr com.apple.quarantine /Applications/Twin.app 2>/dev/null || true
mkdir -p "$HOME/.local/bin"
ln -sfn /Applications/Twin.app/Contents/MacOS/twin-cli "$HOME/.local/bin/twin"
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) say "Note: add ~/.local/bin to your PATH for the twin command";; esac
say "Done. Opening Twin."
open /Applications/Twin.app
