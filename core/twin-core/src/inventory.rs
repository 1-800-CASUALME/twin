use crate::config::Config;
use crate::event::{Emitter, Event};
use crate::{cmd, paths, slug};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub id: String,
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub count: u64,
    pub detail: String,
    pub local: bool,
    pub peer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub bytes: u64,
    pub count: u64,
    pub location: String,
    pub members: Vec<Member>,
}

pub fn dir_size(p: &Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    for e in walkdir::WalkDir::new(p).into_iter().flatten() {
        if e.file_type().is_file() {
            files += 1;
            bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    (bytes, files)
}

fn claude_local(home: &str) -> Item {
    let root = paths::claude_projects_dir();
    let mut members = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let sessions = std::fs::read_dir(e.path())
                .map(|r| r.flatten().filter(|f| f.path().extension().map(|x| x == "jsonl").unwrap_or(false)).count())
                .unwrap_or(0) as u64;
            let (bytes, _) = dir_size(&e.path());
            members.push(Member {
                id: name.clone(),
                name: slug::display_path(&name, home),
                path: e.path().to_string_lossy().into_owned(),
                bytes,
                count: sessions,
                detail: format!("{sessions} sessions"),
                local: true,
                peer: false,
            });
        }
    }
    members.sort_by_key(|m| std::cmp::Reverse(m.bytes));
    finish_item("claude", "Claude", "sparkles", &root.to_string_lossy(), members)
}

pub fn git_repos(home: &Path) -> Vec<PathBuf> {
    const SKIP: &[&str] = &[
        "Library", "node_modules", ".cache", ".local", ".cargo", ".rustup", ".npm", ".claude", ".twin", ".Trash", "go", ".hermes", "target",
    ];
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(home).max_depth(5).into_iter();
    for e in walker
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            e.depth() == 0 || n == ".git" || (!n.starts_with('.') && !SKIP.contains(&n.as_ref()))
        })
        .flatten()
    {
        if e.file_type().is_dir() && e.file_name() == ".git" {
            out.push(e.path().parent().unwrap().to_path_buf());
        }
    }
    out.sort();
    out
}

fn git_local(home: &Path) -> Item {
    let mut members = Vec::new();
    for repo in git_repos(home) {
        let rel = paths::home_relative(&repo).unwrap_or_else(|| repo.to_string_lossy().into_owned());
        let remote = cmd::run("git", &["remote", "get-url", "origin"], Some(&repo))
            .ok()
            .filter(|o| o.status == 0)
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
        let branch = cmd::run("git", &["rev-parse", "--abbrev-ref", "HEAD"], Some(&repo))
            .ok()
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
        let dirty = cmd::run("git", &["status", "--porcelain"], Some(&repo))
            .ok()
            .map(|o| !o.stdout.trim().is_empty())
            .unwrap_or(false);
        let (bytes, _) = dir_size(&repo);
        let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(rel.clone());
        let detail = format!(
            "{}{}{}",
            if remote.is_empty() { "no remote".to_string() } else { remote.clone() },
            if branch.is_empty() { String::new() } else { format!(" · {branch}") },
            if dirty { " · uncommitted changes" } else { "" }
        );
        members.push(Member {
            id: rel.clone(),
            name,
            path: repo.to_string_lossy().into_owned(),
            bytes,
            count: 1,
            detail,
            local: true,
            peer: false,
        });
    }
    finish_item("git", "Git", "arrow.triangle.branch", "~", members)
}

fn finish_item(id: &str, name: &str, icon: &str, location: &str, members: Vec<Member>) -> Item {
    let bytes = members.iter().map(|m| m.bytes).sum();
    let count = members.len() as u64;
    let home = paths::home().to_string_lossy().into_owned();
    Item {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        bytes,
        count,
        location: location.replace(&home, "~"),
        members,
    }
}

fn dotfiles_local() -> Item {
    let mut members = Vec::new();
    let managed = crate::engines::dotfiles::managed();
    let list: Vec<String> = if managed.is_empty() {
        crate::engines::dotfiles::SEED.iter().map(|s| s.to_string()).filter(|s| paths::home().join(s).is_file()).collect()
    } else {
        managed
    };
    for rel in list {
        let p = paths::home().join(&rel);
        let bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        members.push(Member { id: rel.clone(), name: rel.clone(), path: p.to_string_lossy().into_owned(), bytes, count: 1, detail: "chezmoi".into(), local: true, peer: false });
    }
    finish_item("dotfiles", "Dotfiles", "doc.text", "~/.twin/dotfiles", members)
}

fn history_local() -> Item {
    let db = paths::home().join(".local/share/atuin/history.db");
    let (bytes, n) = if db.is_file() {
        let b = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
        let n = cmd::run("atuin", &["stats", "--count", "1"], None).ok().and_then(|o| o.stdout.lines().find(|l| l.contains("Total commands")).and_then(|l| l.split_whitespace().last()?.parse::<u64>().ok())).unwrap_or(0);
        (b, n)
    } else {
        let zh = paths::home().join(".zsh_history");
        (std::fs::metadata(&zh).map(|m| m.len()).unwrap_or(0), std::fs::read_to_string(&zh).map(|s| s.lines().count() as u64).unwrap_or(0))
    };
    let member = Member { id: "shell".into(), name: "Shell history".into(), path: db.to_string_lossy().into_owned(), bytes, count: n, detail: if db.is_file() { "atuin".into() } else { "atuin not set up yet".into() }, local: true, peer: false };
    finish_item("history", "History", "clock.arrow.circlepath", "~/.local/share/atuin", vec![member])
}

fn terminal_local() -> Item {
    let mut members = Vec::new();
    if let Some(dir) = crate::engines::terminal::resurrect_dir() {
        let saves = crate::engines::files::list_files(&dir).into_iter().filter(|f| f.rel.starts_with("tmux_resurrect_")).count() as u64;
        let (bytes, _) = dir_size(&dir);
        members.push(Member { id: "resurrect".into(), name: "tmux layouts".into(), path: dir.to_string_lossy().into_owned(), bytes, count: saves, detail: format!("{saves} saves"), local: true, peer: false });
    }
    finish_item("terminal", "Terminal", "terminal", "~/.local/share/tmux/resurrect", members)
}

fn folders_local(cfg: &Config) -> Item {
    let mut members = Vec::new();
    for rel in crate::engines::folders::folders(cfg) {
        let p = paths::home().join(&rel);
        if !p.is_dir() {
            continue;
        }
        let (bytes, files) = dir_size(&p);
        members.push(Member { id: rel.clone(), name: rel.clone(), path: p.to_string_lossy().into_owned(), bytes, count: files, detail: format!("{files} files"), local: true, peer: false });
    }
    finish_item("folders", "Folders", "folder", "~", members)
}

pub fn local(cfg: &Config) -> Vec<Item> {
    let home = paths::home();
    vec![claude_local(&home.to_string_lossy()), git_local(&home), dotfiles_local(), history_local(), terminal_local(), folders_local(cfg)]
}

pub fn merged(cfg: &Config, emitter: &dyn Emitter) -> Result<Vec<Item>> {
    let mut items = local(cfg);
    if let Ok(peer) = crate::ssh::Peer::new(cfg) {
        if peer.reachable() {
            let local_home = paths::home().to_string_lossy().into_owned();
            let peer_items: Vec<Item> = peer
                .twin_json::<Event>(&["inventory", "--local"])?
                .into_iter()
                .filter_map(|e| if let Event::Item(i) = e { Some(i) } else { None })
                .collect();
            for pi in peer_items {
                let Some(li) = items.iter_mut().find(|i| i.id == pi.id) else { continue };
                for mut pm in pi.members {
                    let key = if pi.id == "claude" {
                        slug::translate(&pm.id, &peer.home, &local_home).unwrap_or(pm.id.clone())
                    } else {
                        pm.id.clone()
                    };
                    if let Some(lm) = li.members.iter_mut().find(|m| m.id == key) {
                        lm.peer = true;
                        if pm.bytes > lm.bytes {
                            lm.bytes = pm.bytes;
                        }
                        if pm.count > lm.count {
                            lm.count = pm.count;
                        }
                    } else {
                        pm.id = key;
                        pm.local = false;
                        pm.peer = true;
                        li.members.push(pm);
                    }
                }
                li.bytes = li.members.iter().map(|m| m.bytes).sum();
                li.count = li.members.len() as u64;
            }
        }
    }
    for i in &items {
        emitter.emit(Event::Item(i.clone()));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_git_repos_and_skips_nested_noise() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/.git")).unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/b/.git")).unwrap();
        std::fs::create_dir_all(d.path().join(".codex/tmp/.git")).unwrap();
        std::fs::create_dir_all(d.path().join("x/y/z/c/.git")).unwrap();
        let r = git_repos(d.path());
        assert_eq!(r, vec![d.path().join("a"), d.path().join("x/y/z/c")]);
    }
    #[test]
    fn dir_size_counts_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("f"), b"12345").unwrap();
        assert_eq!(dir_size(d.path()), (5, 1));
    }
}
