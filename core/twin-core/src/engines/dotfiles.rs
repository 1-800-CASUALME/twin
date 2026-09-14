//! Dotfiles through chezmoi. Source dir is a git repo at ~/.twin/dotfiles synced peer to peer.
use super::git::sync_repo;
use super::Engine;
use crate::cmd::{run, run_ok, which};
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::paths;
use crate::ssh::{shell_quote, Peer, ALIAS};
use anyhow::Result;
use std::path::PathBuf;

pub const SEED: &[&str] = &[
    ".zshrc",
    ".zprofile",
    ".bashrc",
    ".tmux.conf",
    ".gitconfig",
    ".config/atuin/config.toml",
    ".config/ghostty/config",
    ".claude/settings.json",
    ".claude/CLAUDE.md",
];

const IGNORE_TMPL: &str = r#"{{ if ne .chezmoi.os "darwin" }}
.config/karabiner
.config/aerospace
{{ end }}
"#;

pub fn source_dir() -> PathBuf {
    paths::twin_dir().join("dotfiles")
}

/// Files chezmoi manages right now (empty until the first sync).
pub fn managed() -> Vec<String> {
    if !which("chezmoi") {
        return vec![];
    }
    run("chezmoi", &["--source", source_dir().to_str().unwrap(), "managed", "--include", "files"], None)
        .map(|o| o.stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

fn chez(args: &[&str]) -> Result<String> {
    let src = source_dir();
    let mut a = vec!["--source", src.to_str().unwrap()];
    a.extend_from_slice(args);
    run_ok("chezmoi", &a, None)
}

fn ensure_source_repo() -> Result<()> {
    let src = source_dir();
    std::fs::create_dir_all(&src)?;
    if !src.join(".git").is_dir() {
        run_ok("git", &["init", "-q"], Some(&src))?;
        run_ok("git", &["config", "receive.denyCurrentBranch", "updateInstead"], Some(&src))?;
    }
    let ig = src.join(".chezmoiignore");
    if !ig.exists() {
        std::fs::write(&ig, IGNORE_TMPL)?;
    }
    Ok(())
}

fn seed_files() -> Result<usize> {
    let mut n = 0;
    for rel in SEED {
        let p = paths::home().join(rel);
        if p.is_file() && chez(&["add", p.to_str().unwrap()]).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

pub struct DotfilesEngine;

impl Engine for DotfilesEngine {
    fn id(&self) -> &'static str {
        "dotfiles"
    }
    fn sync(&self, _cfg: &Config, peer: &Peer, _members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "dotfiles";
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: "chezmoi".into() });
        if !which("chezmoi") {
            emitter.emit(Event::Step { id: id.into(), state: State::Warn, msg: "chezmoi is not installed here (Fix it on the Diagnose screen)".into() });
            return Ok(());
        }
        let peer_has = peer.sh("command -v chezmoi >/dev/null").map(|o| o.status == 0).unwrap_or(false);
        if !peer_has {
            emitter.emit(Event::Step { id: id.into(), state: State::Warn, msg: format!("chezmoi is not installed on {}", peer.name) });
            return Ok(());
        }
        ensure_source_repo()?;
        let host = crate::discover::local_info().host;
        let src = source_dir();
        let first = run("git", &["rev-parse", "--verify", "-q", "HEAD"], Some(&src)).map(|o| o.status != 0).unwrap_or(true);
        let seeded = if first { seed_files()? } else { 0 };
        // capture local edits to managed files back into the source dir
        let _ = chez(&["re-add"]);
        // the peer's source dir is our origin
        let remote_src = format!("{}/.twin/dotfiles", peer.home.trim_end_matches('/'));
        let peer_setup = format!(
            "mkdir -p {d} && cd {d} && {{ [ -d .git ] || git init -q; }} && git config receive.denyCurrentBranch updateInstead",
            d = shell_quote(&remote_src)
        );
        let o = peer.sh(&peer_setup)?;
        if o.status != 0 {
            emitter.emit(Event::Step { id: id.into(), state: State::Fail, msg: format!("could not prepare dotfiles repo on {}: {}", peer.name, o.stderr.trim()) });
            return Ok(());
        }
        // capture and commit the peer's edits first, so our fetch below sees them
        let peer_capture = format!(
            "chezmoi --source {d} re-add >/dev/null 2>&1; twin git-sync-local --autocommit -- .twin/dotfiles >/dev/null 2>&1; true",
            d = shell_quote(&remote_src)
        );
        let _ = peer.sh(&peer_capture);
        let url = if peer.is_local() { remote_src.clone() } else { format!("{ALIAS}:.twin/dotfiles") };
        let has_origin = run("git", &["remote", "get-url", "origin"], Some(&src)).map(|o| o.status == 0).unwrap_or(false);
        if has_origin {
            run_ok("git", &["remote", "set-url", "origin", &url], Some(&src))?;
        } else {
            run_ok("git", &["remote", "add", "origin", &url], Some(&src))?;
        }
        let out = sync_repo(&src, true, &host)?;
        let st = if out.action == "refused" { State::Warn } else { State::Ok };
        emitter.emit(Event::Step { id: format!("{id}:repo"), state: st, msg: out.msg.clone() });
        // apply on both sides
        let applied_here = chez(&["apply", "--force"]).is_ok();
        let peer_apply = format!("cd {} && git checkout -q -- . 2>/dev/null; chezmoi --source {} apply --force", shell_quote(&remote_src), shell_quote(&remote_src));
        let applied_peer = peer.sh(&peer_apply).map(|o| o.status == 0).unwrap_or(false);
        let n = managed().len();
        emitter.emit(Event::Step {
            id: id.into(),
            state: if applied_here && applied_peer { State::Ok } else { State::Warn },
            msg: format!(
                "{n} files managed{}{}{}",
                if seeded > 0 { format!(", {seeded} added") } else { String::new() },
                if applied_here { "" } else { ", apply failed here" },
                if applied_peer { "" } else { ", apply failed on the other machine" }
            ),
        });
        Ok(())
    }
}
