#!/usr/bin/env bash
# Integration test: two fake home directories on one machine, synced through
# twin's local transport (TWIN_PEER_LOCAL=1). Exercises the Claude engine
# (union of projects, larger transcript wins, slug translation) and the Git
# engine (bare repo on the hub, clone at the mirrored path, round trip).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q -p twin
TWIN="$PWD/target/debug/twin"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
A="$T/homeA"; B="$T/homeB"
mkdir -p "$A/.claude/projects/$(echo "$A" | tr -c 'A-Za-z0-9\n' '-')-p1" \
         "$B/.claude/projects/$(echo "$B" | tr -c 'A-Za-z0-9\n' '-')-p1" \
         "$B/.claude/projects/$(echo "$B" | tr -c 'A-Za-z0-9\n' '-')-p2"
SA="$(echo "$A" | tr -c 'A-Za-z0-9\n' '-')"; SB="$(echo "$B" | tr -c 'A-Za-z0-9\n' '-')"
printf 'line1\nline2\n'        > "$A/.claude/projects/$SA-p1/s1.jsonl"
printf 'line1\nline2\nline3\n' > "$B/.claude/projects/$SB-p1/s1.jsonl"
printf 'only-b\n'              > "$B/.claude/projects/$SB-p2/s2.jsonl"
mkdir -p "$A/.claude/projects/$SA-p1/memory" "$B/.claude/projects/$SB-p1/memory"
printf 'old\n' > "$A/.claude/projects/$SA-p1/memory/MEMORY.md"; touch -t 202001010000 "$A/.claude/projects/$SA-p1/memory/MEMORY.md"
printf 'new\n' > "$B/.claude/projects/$SB-p1/memory/MEMORY.md"
# git repo only in A, no remote; B is the hub
( cd "$A" && mkdir -p code/r && cd code/r && git init -q && git config user.email t@t && git config user.name t && echo hi > f && git add . && git commit -qm init )
mkdir -p "$A/.twin" "$B/.twin"
cat > "$A/.twin/config.toml" <<EOT
git_autocommit = ["code/r"]
[peer]
name = "peerB"
host = "localhost"
os = "linux"
user = "$USER"
home = "$B"
addr = "127.0.0.1"
hub = true
EOT
# the peer-side twin must resolve to this build
mkdir -p "$T/bin"; ln -s "$TWIN" "$T/bin/twin"
export TWIN_PEER_LOCAL=1 TWIN_PEER_PATH_PREFIX="$T/bin"
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
HOME="$A" "$TWIN" sync claude git | tee "$T/out.jsonl"
grep -q '"ev":"done","ok":true' "$T/out.jsonl"
# Claude: larger jsonl wins, union of projects, newer memory wins and loser kept
[ "$(cat "$A/.claude/projects/$SA-p1/s1.jsonl")" = "$(printf 'line1\nline2\nline3\n')" ]
[ -f "$A/.claude/projects/$SA-p2/s2.jsonl" ]
[ "$(cat "$A/.claude/projects/$SA-p1/memory/MEMORY.md")" = "new" ]
ls "$A/.claude/projects/$SA-p1/memory/" | grep -q 'MEMORY.twin-conflict-'
# Git: bare repo created on hub (B), B has a clone at the same relative path, same HEAD
[ -d "$B/git/r.git" ]
[ -d "$B/code/r/.git" ]
[ "$(git -C "$A/code/r" rev-parse HEAD)" = "$(git -C "$B/code/r" rev-parse HEAD)" ]
# Round trip: edit on B, sync from A again, A fast-forwards
echo more >> "$B/code/r/f"
( cd "$B/code/r" && git commit -qam "edit on B" && git push -q origin HEAD )
HOME="$A" "$TWIN" sync git > "$T/out2.jsonl"
grep -q 'commits pulled' "$T/out2.jsonl"
[ "$(git -C "$A/code/r" rev-parse HEAD)" = "$(git -C "$B/code/r" rev-parse HEAD)" ]
echo "two-homes: OK"
