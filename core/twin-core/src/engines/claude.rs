use super::Engine;
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::ssh::{shell_quote, Peer};
use crate::{paths, slug};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub rel: String,
    pub size: u64,
    pub mtime: i64,
}

pub fn list_files(root: &Path) -> Vec<FileEntry> {
    let mut v = Vec::new();
    for e in walkdir::WalkDir::new(root).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(root).unwrap().to_string_lossy().into_owned();
        if rel.contains(".twin-conflict-") {
            continue;
        }
        let m = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        v.push(FileEntry { rel, size: m.len(), mtime });
    }
    v.sort_by(|a, b| a.rel.cmp(&b.rel));
    v
}

#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// local -> peer
    pub push: Vec<String>,
    /// peer -> local
    pub pull: Vec<String>,
    /// rename local copy before pull
    pub conflict_local: Vec<String>,
    /// rename peer copy before push
    pub conflict_peer: Vec<String>,
    /// jsonl that need a prefix hash check before overwrite (rel, min size)
    pub prefix_check: Vec<(String, u64)>,
}

pub fn plan(local: &[FileEntry], peer: &[FileEntry]) -> Plan {
    let l: HashMap<&str, &FileEntry> = local.iter().map(|f| (f.rel.as_str(), f)).collect();
    let p: HashMap<&str, &FileEntry> = peer.iter().map(|f| (f.rel.as_str(), f)).collect();
    let mut plan = Plan::default();
    for f in local {
        if !p.contains_key(f.rel.as_str()) {
            plan.push.push(f.rel.clone());
        }
    }
    for f in peer {
        if !l.contains_key(f.rel.as_str()) {
            plan.pull.push(f.rel.clone());
        }
    }
    for f in local {
        let Some(g) = p.get(f.rel.as_str()) else { continue };
        if f.size == g.size && (f.rel.ends_with(".jsonl") || f.mtime == g.mtime) {
            continue;
        }
        if f.rel.ends_with(".jsonl") {
            plan.prefix_check.push((f.rel.clone(), f.size.min(g.size)));
            if f.size > g.size {
                plan.push.push(f.rel.clone());
            } else {
                plan.pull.push(f.rel.clone());
            }
        } else if f.mtime > g.mtime {
            plan.conflict_peer.push(f.rel.clone());
            plan.push.push(f.rel.clone());
        } else {
            // peer newer, or same mtime with different size: keep both, take peer's
            plan.conflict_local.push(f.rel.clone());
            plan.pull.push(f.rel.clone());
        }
    }
    plan
}

pub fn conflict_name(rel: &str, host: &str) -> String {
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    match rel.rfind('.') {
        Some(i) if !rel[i..].contains('/') => format!("{}.twin-conflict-{host}-{ts}{}", &rel[..i], &rel[i..]),
        _ => format!("{rel}.twin-conflict-{host}-{ts}"),
    }
}

pub fn prefix_hash(path: &Path, n: u64) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut left = n;
    while left > 0 {
        let want = buf.len().min(left as usize);
        let got = f.read(&mut buf[..want])?;
        if got == 0 {
            break;
        }
        h.update(&buf[..got]);
        left -= got as u64;
    }
    Ok(hex::encode(h.finalize()))
}

pub fn conflict_copy(abs: &Path, host: &str) -> Result<String> {
    let name = abs.file_name().unwrap().to_string_lossy().into_owned();
    let new = abs.with_file_name(conflict_name(&name, host));
    std::fs::rename(abs, &new).with_context(|| format!("rename {}", abs.display()))?;
    Ok(new.to_string_lossy().into_owned())
}

pub struct ClaudeEngine;

impl Engine for ClaudeEngine {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn sync(&self, _cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "claude";
        let local_home = paths::home().to_string_lossy().into_owned();
        let local_host = crate::discover::local_info().host;
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
        let (mut pushed, mut pulled, mut conflicts) = (0u64, 0u64, 0u64);
        for (i, s) in slugs.iter().enumerate() {
            let Some(peer_slug) = slug::translate(s, &local_home, &peer.home) else { continue };
            let local_dir = root.join(s);
            let peer_dir = format!("{peer_root}/{peer_slug}");
            let local_files = if local_dir.is_dir() { list_files(&local_dir) } else { vec![] };
            let peer_files: Vec<FileEntry> = peer.twin_json(&["claude-files", &peer_slug])?;
            let mut plan = plan(&local_files, &peer_files);
            if !plan.prefix_check.is_empty() {
                let input: String = plan.prefix_check.iter().map(|(r, n)| format!("{r}\t{n}\n")).collect();
                let remote = peer_prefix_hashes(peer, &peer_slug, &input)?;
                for (rel, n) in plan.prefix_check.clone() {
                    let mine = prefix_hash(&local_dir.join(&rel), n).unwrap_or_default();
                    if remote.get(&rel).map(|h| h != &mine).unwrap_or(true) {
                        if plan.push.contains(&rel) {
                            plan.conflict_peer.push(rel.clone());
                        } else {
                            plan.conflict_local.push(rel.clone());
                        }
                    }
                }
            }
            for rel in &plan.conflict_local {
                let kept = conflict_copy(&local_dir.join(rel), &local_host)?;
                conflicts += 1;
                emitter.emit(Event::Conflict { id: id.into(), path: format!("{s}/{rel}"), kept });
            }
            for rel in &plan.conflict_peer {
                let abs = format!("{peer_dir}/{rel}");
                let _ = peer.run(&["twin", "conflict-copy", &abs, &peer.name]);
                conflicts += 1;
                emitter.emit(Event::Conflict { id: id.into(), path: format!("{s}/{rel}"), kept: format!("{}:{abs}", peer.name) });
            }
            if !plan.push.is_empty() {
                let tmp = write_list(&plan.push)?;
                peer.rsync_push(&local_dir, &peer_dir, tmp.path())?;
                pushed += plan.push.len() as u64;
            }
            if !plan.pull.is_empty() {
                let tmp = write_list(&plan.pull)?;
                peer.rsync_pull(&peer_dir, &local_dir, tmp.path())?;
                pulled += plan.pull.len() as u64;
            }
            emitter.emit(Event::Progress { id: id.into(), done: i as u64 + 1, total, bytes: 0 });
        }
        emitter.emit(Event::Step {
            id: id.into(),
            state: State::Ok,
            msg: format!(
                "{pushed} files to {}, {pulled} files here{}",
                peer.name,
                if conflicts > 0 { format!(", {conflicts} conflicts kept") } else { String::new() }
            ),
        });
        Ok(())
    }
}

fn write_list(rels: &[String]) -> Result<tempfile::NamedTempFile> {
    let mut f = tempfile::NamedTempFile::new()?;
    for r in rels {
        writeln!(f, "{r}")?;
    }
    f.flush()?;
    Ok(f)
}

fn peer_prefix_hashes(peer: &Peer, peer_slug: &str, input: &str) -> Result<HashMap<String, String>> {
    let script = format!("printf %s {} | twin claude-prefix-hash {}", shell_quote(input), shell_quote(peer_slug));
    let o = peer.sh(&script)?;
    Ok(o
        .stdout
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(2, '\t');
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fe(rel: &str, size: u64, mtime: i64) -> FileEntry {
        FileEntry { rel: rel.into(), size, mtime }
    }
    #[test]
    fn plan_jsonl_larger_wins() {
        let p = plan(&[fe("a.jsonl", 100, 1)], &[fe("a.jsonl", 200, 5)]);
        assert_eq!(p.pull, vec!["a.jsonl"]);
        assert!(p.push.is_empty());
        assert_eq!(p.prefix_check, vec![("a.jsonl".to_string(), 100)]);
        assert!(p.conflict_local.is_empty());
    }
    #[test]
    fn plan_jsonl_equal_size_skips_even_if_mtime_differs() {
        let p = plan(&[fe("a.jsonl", 100, 1)], &[fe("a.jsonl", 100, 9)]);
        assert_eq!(p, Plan::default());
    }
    #[test]
    fn plan_md_newer_wins_and_loser_is_conflict() {
        let p = plan(&[fe("memory/MEMORY.md", 10, 100)], &[fe("memory/MEMORY.md", 12, 50)]);
        assert_eq!(p.push, vec!["memory/MEMORY.md"]);
        assert_eq!(p.conflict_peer, vec!["memory/MEMORY.md"]);
        let p = plan(&[fe("memory/MEMORY.md", 10, 50)], &[fe("memory/MEMORY.md", 12, 100)]);
        assert_eq!(p.pull, vec!["memory/MEMORY.md"]);
        assert_eq!(p.conflict_local, vec!["memory/MEMORY.md"]);
    }
    #[test]
    fn plan_only_one_side_copies() {
        let p = plan(&[fe("x.jsonl", 1, 1)], &[fe("y.jsonl", 1, 1)]);
        assert_eq!(p.push, vec!["x.jsonl"]);
        assert_eq!(p.pull, vec!["y.jsonl"]);
    }
    #[test]
    fn conflict_name_keeps_extension() {
        let n = conflict_name("memory/MEMORY.md", "mac");
        assert!(n.starts_with("memory/MEMORY.twin-conflict-mac-"));
        assert!(n.ends_with(".md"));
    }
    #[test]
    fn prefix_hash_matches_for_shared_prefix() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        std::fs::write(&a, b"hello world").unwrap();
        std::fs::write(&b, b"hello there").unwrap();
        assert_eq!(prefix_hash(&a, 5).unwrap(), prefix_hash(&b, 5).unwrap());
        assert_ne!(prefix_hash(&a, 8).unwrap(), prefix_hash(&b, 8).unwrap());
    }
    #[test]
    fn list_files_skips_conflict_copies() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.jsonl"), b"x").unwrap();
        std::fs::write(d.path().join("a.twin-conflict-mac-1.jsonl"), b"x").unwrap();
        assert_eq!(list_files(d.path()).len(), 1);
    }
}
