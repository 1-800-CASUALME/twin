use super::files::{sync_dir, Counts};
use super::Engine;
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::ssh::Peer;
use crate::{paths, slug};
use anyhow::Result;

pub use super::files::{conflict_copy, list_files, prefix_hash, FileEntry};

pub struct ClaudeEngine;

impl Engine for ClaudeEngine {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn sync(&self, _cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "claude";
        let local_home = paths::home().to_string_lossy().into_owned();
        let root = paths::claude_projects_dir();
        let mut slugs: Vec<String> = std::fs::read_dir(&root)
            .map(|r| {
                r.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let peer_root = format!("{}/.claude/projects", peer.home.trim_end_matches('/'));
        let peer_slugs: Vec<String> = peer
            .run(&["ls", "-1", &peer_root])
            .map(|o| o.stdout.lines().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        for ps in peer_slugs {
            if let Some(t) = slug::translate(&ps, &peer.home, &local_home) {
                if !slugs.contains(&t) {
                    slugs.push(t);
                }
            }
        }
        if !members.is_empty() {
            slugs.retain(|s| members.contains(s));
        }
        slugs.sort();
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: format!("{} projects", slugs.len()) });
        let total = slugs.len() as u64;
        let mut all = Counts::default();
        for (i, s) in slugs.iter().enumerate() {
            let Some(peer_slug) = slug::translate(s, &local_home, &peer.home) else { continue };
            let local_dir = root.join(s);
            let peer_dir = format!("{peer_root}/{peer_slug}");
            let c = sync_dir(peer, id, s, &local_dir, &peer_dir, &[], emitter)?;
            all.pushed += c.pushed;
            all.pulled += c.pulled;
            all.conflicts += c.conflicts;
            emitter.emit(Event::Progress { id: id.into(), done: i as u64 + 1, total, bytes: 0 });
        }
        emitter.emit(Event::Step {
            id: id.into(),
            state: State::Ok,
            msg: format!(
                "{} files to {}, {} files here{}",
                all.pushed,
                peer.name,
                all.pulled,
                if all.conflicts > 0 { format!(", {} conflicts kept", all.conflicts) } else { String::new() }
            ),
        });
        Ok(())
    }
}
