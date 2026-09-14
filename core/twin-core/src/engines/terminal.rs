//! tmux-resurrect save files, synced both ways with home paths rewritten.
use super::files::{sync_dir, list_files};
use super::Engine;
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::paths;
use crate::ssh::Peer;
use anyhow::Result;
use std::path::PathBuf;

/// Where tmux-resurrect keeps saves on this machine, if set up.
pub fn resurrect_dir() -> Option<PathBuf> {
    let h = paths::home();
    [h.join(".local/share/tmux/resurrect"), h.join(".tmux/resurrect")].into_iter().find(|p| p.is_dir())
}

pub fn resurrect_dir_for(home: &str) -> String {
    format!("{}/.local/share/tmux/resurrect", home.trim_end_matches('/'))
}

/// Rewrite the peer's home prefix to ours inside every save file, and point `last` at the newest.
pub fn localize(dir: &std::path::Path, peer_home: &str) -> Result<usize> {
    let local_home = paths::home().to_string_lossy().into_owned();
    let mut n = 0;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for f in list_files(dir) {
        if !f.rel.starts_with("tmux_resurrect_") || !f.rel.ends_with(".txt") {
            continue;
        }
        let p = dir.join(&f.rel);
        let s = std::fs::read_to_string(&p)?;
        if s.contains(peer_home) {
            std::fs::write(&p, s.replace(peer_home, &local_home))?;
            n += 1;
        }
        if let Ok(m) = std::fs::metadata(&p).and_then(|m| m.modified()) {
            if newest.as_ref().map(|(t, _)| m > *t).unwrap_or(true) {
                newest = Some((m, p.clone()));
            }
        }
    }
    if let Some((_, p)) = newest {
        let last = dir.join("last");
        let _ = std::fs::remove_file(&last);
        #[cfg(unix)]
        std::os::unix::fs::symlink(p.file_name().unwrap(), &last)?;
    }
    Ok(n)
}

pub struct TerminalEngine;

impl Engine for TerminalEngine {
    fn id(&self) -> &'static str {
        "terminal"
    }
    fn sync(&self, _cfg: &Config, peer: &Peer, _members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "terminal";
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: "tmux layouts".into() });
        let local = resurrect_dir().unwrap_or_else(|| paths::home().join(".local/share/tmux/resurrect"));
        let remote = resurrect_dir_for(&peer.home);
        let remote_exists = peer.run(&["test", "-d", &remote]).map(|o| o.status == 0).unwrap_or(false);
        if !local.is_dir() && !remote_exists {
            emitter.emit(Event::Step {
                id: id.into(),
                state: State::Warn,
                msg: "tmux-resurrect is not set up on either machine (install tpm + tmux-resurrect, then prefix+Ctrl-s saves a layout)".into(),
            });
            return Ok(());
        }
        std::fs::create_dir_all(&local)?;
        let c = sync_dir(peer, id, "resurrect", &local, &remote, &["last"], emitter)?;
        let rewrote = localize(&local, &peer.home)?;
        let peer_script = format!("twin localize-resurrect -- {}", crate::ssh::shell_quote(&paths::home().to_string_lossy()));
        let _ = peer.sh(&peer_script);
        emitter.emit(Event::Step {
            id: id.into(),
            state: State::Ok,
            msg: format!("{} saves to {}, {} saves here, {} paths rewritten; attach with: twin attach", c.pushed, peer.name, c.pulled, rewrote),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn localize_rewrites_home_and_points_last() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("tmux_resurrect_20260101T000000.txt");
        std::fs::write(&f, "pane\tmain\t0\t:\t0\t:zsh\t1\t:*\t0\t:/home/asim/wagt\t1\tzsh\t:\n").unwrap();
        let n = localize(d.path(), "/home/asim").unwrap();
        assert_eq!(n, 1);
        let s = std::fs::read_to_string(&f).unwrap();
        assert!(s.contains(&paths::home().to_string_lossy().to_string()));
        assert!(!s.contains("/home/asim/wagt"));
        assert!(d.path().join("last").exists());
    }
}
