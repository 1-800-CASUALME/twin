//! Shared directory comparison and transfer used by the Claude, Folders and Terminal engines.
use crate::event::{Emitter, Event};
use crate::ssh::{shell_quote, Peer};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

pub const DEFAULT_IGNORE: &[&str] = &["node_modules", ".venv", "target", "dist", ".next", "__pycache__", ".DS_Store", ".git"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub rel: String,
    pub size: u64,
    pub mtime: i64,
}

pub fn list_files(root: &Path) -> Vec<FileEntry> {
    list_files_ignoring(root, &[])
}

pub fn list_files_ignoring(root: &Path, ignore: &[&str]) -> Vec<FileEntry> {
    let mut v = Vec::new();
    let walker = walkdir::WalkDir::new(root).into_iter();
    for e in walker.filter_entry(|e| e.depth() == 0 || !ignore.contains(&e.file_name().to_string_lossy().as_ref())).flatten() {
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


#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub pushed: u64,
    pub pulled: u64,
    pub conflicts: u64,
}

pub fn write_list(rels: &[String]) -> Result<tempfile::NamedTempFile> {
    let mut f = tempfile::NamedTempFile::new()?;
    for r in rels {
        writeln!(f, "{r}")?;
    }
    f.flush()?;
    Ok(f)
}

fn peer_prefix_hashes(peer: &Peer, remote_dir: &str, input: &str) -> Result<HashMap<String, String>> {
    let script = format!("printf %s {} | twin prefix-hash -- {}", shell_quote(input), shell_quote(remote_dir));
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

/// Compare one local directory with one remote directory and move files both ways.
/// `label` prefixes conflict paths in events; `id` is the engine id for events.
pub fn sync_dir(peer: &Peer, id: &str, label: &str, local_dir: &Path, remote_dir: &str, ignore: &[&str], emitter: &dyn Emitter) -> Result<Counts> {
    let local_host = crate::discover::local_info().host;
    let local_files = if local_dir.is_dir() { list_files_ignoring(local_dir, ignore) } else { vec![] };
    let mut args = vec!["list-files", "--"];
    args.push(remote_dir);
    let ignore_owned: Vec<String> = ignore.iter().map(|s| s.to_string()).collect();
    let mut full_args: Vec<&str> = vec!["list-files"];
    for ig in &ignore_owned {
        full_args.push("--ignore");
        full_args.push(ig);
    }
    full_args.push("--");
    full_args.push(remote_dir);
    let peer_files: Vec<FileEntry> = peer.twin_json(&full_args)?;
    let mut plan = plan(&local_files, &peer_files);
    let mut c = Counts::default();
    if !plan.prefix_check.is_empty() {
        let input: String = plan.prefix_check.iter().map(|(r, n)| format!("{r}\t{n}\n")).collect();
        let remote = peer_prefix_hashes(peer, remote_dir, &input)?;
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
        c.conflicts += 1;
        emitter.emit(Event::Conflict { id: id.into(), path: format!("{label}/{rel}"), kept });
    }
    for rel in &plan.conflict_peer {
        let abs = format!("{remote_dir}/{rel}");
        let _ = peer.run(&["twin", "conflict-copy", "--", &abs, &peer.name]);
        c.conflicts += 1;
        emitter.emit(Event::Conflict { id: id.into(), path: format!("{label}/{rel}"), kept: format!("{}:{abs}", peer.name) });
    }
    if !plan.push.is_empty() {
        let tmp = write_list(&plan.push)?;
        peer.rsync_push(local_dir, remote_dir, tmp.path())?;
        c.pushed += plan.push.len() as u64;
    }
    if !plan.pull.is_empty() {
        let tmp = write_list(&plan.pull)?;
        peer.rsync_pull(remote_dir, local_dir, tmp.path())?;
        c.pulled += plan.pull.len() as u64;
    }
    Ok(c)
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

#[cfg(test)]
mod ignore_tests {
    use super::*;
    #[test]
    fn list_files_ignoring_skips_named_dirs() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/x")).unwrap();
        std::fs::write(d.path().join("node_modules/x/a.js"), b"x").unwrap();
        std::fs::write(d.path().join("keep.txt"), b"x").unwrap();
        let v = list_files_ignoring(d.path(), DEFAULT_IGNORE);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rel, "keep.txt");
    }
}
