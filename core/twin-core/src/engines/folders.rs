//! Non-repo folders, rsync both ways, newest wins, losers kept as conflict copies.
use super::files::{sync_dir, Counts, DEFAULT_IGNORE};
use super::Engine;
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::paths;
use crate::ssh::Peer;
use anyhow::Result;

pub const SEED: &[&str] = &["Sanada", "NabdaTech"];

/// Folders to sync: configured ones, plus seeds that exist locally when nothing is configured.
pub fn folders(cfg: &Config) -> Vec<String> {
    if !cfg.folders.is_empty() {
        return cfg.folders.clone();
    }
    SEED.iter().filter(|f| paths::home().join(f).is_dir()).map(|s| s.to_string()).collect()
}

pub struct FoldersEngine;

impl Engine for FoldersEngine {
    fn id(&self) -> &'static str {
        "folders"
    }
    fn sync(&self, cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "folders";
        let mut list = folders(cfg);
        if !members.is_empty() {
            list.retain(|f| members.contains(f));
        }
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: format!("{} folders", list.len()) });
        let total = list.len() as u64;
        let mut all = Counts::default();
        let mut skipped = 0;
        for (i, rel) in list.iter().enumerate() {
            let local_dir = paths::home().join(rel);
            if local_dir.join(".git").is_dir() {
                emitter.emit(Event::Step { id: format!("{id}:{rel}"), state: State::Skipped, msg: "is a git repo, handled under Git".into() });
                skipped += 1;
                continue;
            }
            let remote_dir = format!("{}/{rel}", peer.home.trim_end_matches('/'));
            let c = sync_dir(peer, id, rel, &local_dir, &remote_dir, DEFAULT_IGNORE, emitter)?;
            all.pushed += c.pushed;
            all.pulled += c.pulled;
            all.conflicts += c.conflicts;
            emitter.emit(Event::Step { id: format!("{id}:{rel}"), state: State::Ok, msg: format!("{} out, {} in", c.pushed, c.pulled) });
            emitter.emit(Event::Progress { id: id.into(), done: i as u64 + 1, total, bytes: 0 });
        }
        emitter.emit(Event::Step {
            id: id.into(),
            state: State::Ok,
            msg: format!(
                "{} files to {}, {} files here{}{}",
                all.pushed,
                peer.name,
                all.pulled,
                if all.conflicts > 0 { format!(", {} conflicts kept", all.conflicts) } else { String::new() },
                if skipped > 0 { format!(", {skipped} skipped") } else { String::new() }
            ),
        });
        Ok(())
    }
}
