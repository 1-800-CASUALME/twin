use super::Engine;
use crate::cmd::{run, run_ok};
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::ssh::{shell_quote, Peer};
use crate::{inventory, paths};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoState {
    pub branch: String,
    pub detached: bool,
    pub in_progress: bool,
    pub dirty: bool,
    pub has_remote: bool,
    pub has_upstream: bool,
    pub ahead: u64,
    pub behind: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Outcome {
    pub action: String,
    pub msg: String,
}

fn g(repo: &Path, args: &[&str]) -> Result<String> {
    run_ok("git", args, Some(repo))
}
fn g_ok(repo: &Path, args: &[&str]) -> bool {
    run("git", args, Some(repo)).map(|o| o.status == 0).unwrap_or(false)
}

pub fn state(repo: &Path) -> Result<RepoState> {
    let has_head = g_ok(repo, &["rev-parse", "--verify", "-q", "HEAD"]);
    let branch = if has_head {
        g(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string()
    } else {
        // unborn branch (no commits yet)
        g(repo, &["symbolic-ref", "--short", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_else(|_| "main".into())
    };
    let detached = branch == "HEAD";
    let gd = repo.join(".git");
    let in_progress = gd.join("MERGE_HEAD").exists() || gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists();
    let dirty = !g(repo, &["status", "--porcelain"])?.trim().is_empty();
    let has_remote = g_ok(repo, &["remote", "get-url", "origin"]);
    let has_upstream = has_head && has_remote && !detached && g_ok(repo, &["rev-parse", "--verify", "-q", &format!("origin/{branch}")]);
    let (ahead, behind) = if has_upstream {
        let s = g(repo, &["rev-list", "--left-right", "--count", &format!("HEAD...origin/{branch}")])?;
        let mut it = s.split_whitespace().map(|x| x.parse::<u64>().unwrap_or(0));
        (it.next().unwrap_or(0), it.next().unwrap_or(0))
    } else {
        (0, 0)
    };
    Ok(RepoState { branch, detached, in_progress, dirty, has_remote, has_upstream, ahead, behind })
}

fn refused(msg: &str) -> Outcome {
    Outcome { action: "refused".into(), msg: msg.into() }
}

/// One git-sync pass. Does not create remotes; callers do that first.
pub fn sync_repo(repo: &Path, autocommit: bool, host: &str) -> Result<Outcome> {
    let st = state(repo)?;
    if st.detached {
        return Ok(refused("detached HEAD"));
    }
    if st.in_progress {
        return Ok(refused("merge or rebase in progress"));
    }
    if st.dirty {
        if !autocommit {
            return Ok(refused("uncommitted changes (auto-commit is off for this repo)"));
        }
        g(repo, &["add", "-A"])?;
        g(repo, &["commit", "-qm", &format!("twin: {host} {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))])?;
    }
    if !g_ok(repo, &["rev-parse", "--verify", "-q", "HEAD"]) {
        return Ok(Outcome { action: "clean".into(), msg: "nothing to sync yet".into() });
    }
    if !st.has_remote {
        return Ok(Outcome { action: "no-remote".into(), msg: "no origin remote".into() });
    }
    let fetch = run("git", &["fetch", "-q", "origin"], Some(repo))?;
    if fetch.status != 0 {
        return Ok(refused(&format!("fetch failed: {}", fetch.stderr.trim())));
    }
    let st = state(repo)?;
    if !st.has_upstream {
        g(repo, &["push", "-q", "-u", "origin", &st.branch]).context("initial push")?;
        return Ok(Outcome { action: "pushed".into(), msg: format!("published {}", st.branch) });
    }
    match (st.ahead, st.behind) {
        (0, 0) => Ok(Outcome { action: "clean".into(), msg: "up to date".into() }),
        (a, 0) => {
            g(repo, &["push", "-q", "origin", &st.branch])?;
            Ok(Outcome { action: "pushed".into(), msg: format!("{a} commits pushed") })
        }
        (0, b) => {
            g(repo, &["merge", "-q", "--ff-only", &format!("origin/{}", st.branch)])?;
            Ok(Outcome { action: "fast-forwarded".into(), msg: format!("{b} commits pulled") })
        }
        (a, b) => {
            let rb = run("git", &["rebase", "-q", &format!("origin/{}", st.branch)], Some(repo))?;
            if rb.status != 0 {
                let _ = run("git", &["rebase", "--abort"], Some(repo));
                return Ok(refused(&format!("diverged ({a} local, {b} remote) and rebase conflicts; resolve by hand")));
            }
            g(repo, &["push", "-q", "origin", &st.branch])?;
            Ok(Outcome { action: "rebased-and-pushed".into(), msg: format!("{a} local rebased onto {b} remote") })
        }
    }
}

/// Create (if needed) a bare repo on the hub at ~/git/<name>.git. Returns the remote URL usable from the non-hub side.
pub fn ensure_bare_on_hub(peer: &Peer, name: &str, branch: &str) -> Result<String> {
    let path = format!("{}/git/{name}.git", peer.home.trim_end_matches('/'));
    let script = format!(
        "mkdir -p ~/git && {{ [ -d {p} ] || git init -q --bare {p}; }} && git --git-dir={p} symbolic-ref HEAD refs/heads/{b}",
        p = shell_quote(&path),
        b = branch
    );
    let o = peer.sh(&script)?;
    if o.status != 0 {
        bail!("could not create bare repo on {}: {}", peer.name, o.stderr.trim());
    }
    if peer.is_local() {
        return Ok(path);
    }
    Ok(format!("{}:git/{name}.git", crate::ssh::ALIAS))
}

pub fn ensure_bare_local(name: &str, branch: &str) -> Result<String> {
    let dir = paths::home().join("git");
    std::fs::create_dir_all(&dir)?;
    let p = dir.join(format!("{name}.git"));
    if !p.is_dir() {
        run_ok("git", &["init", "-q", "--bare", p.to_str().unwrap()], None)?;
    }
    run_ok("git", &["--git-dir", p.to_str().unwrap(), "symbolic-ref", "HEAD", &format!("refs/heads/{branch}")], None)?;
    Ok(p.to_string_lossy().into_owned())
}

pub struct GitEngine;

impl Engine for GitEngine {
    fn id(&self) -> &'static str {
        "git"
    }
    fn sync(&self, cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "git";
        let host = crate::discover::local_info().host;
        let i_am_hub = cfg.peer.as_ref().map(|p| !p.hub).unwrap_or(false);
        let mut repos = inventory::git_repos(&paths::home());
        if !members.is_empty() {
            repos.retain(|r| members.contains(&paths::home_relative(r).unwrap_or_default()));
        }
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: format!("{} repos", repos.len()) });
        let total = repos.len() as u64;
        let mut refused_n = 0;
        for (i, repo) in repos.iter().enumerate() {
            let rel = paths::home_relative(repo).unwrap_or_default();
            let name = repo.file_name().unwrap().to_string_lossy().into_owned();
            let auto = cfg.git_autocommit.contains(&rel);
            let st = state(repo)?;
            if !st.has_remote {
                let url = if i_am_hub { ensure_bare_local(&name, &st.branch)? } else { ensure_bare_on_hub(peer, &name, &st.branch)? };
                g(repo, &["remote", "add", "origin", &url])?;
            }
            let out = sync_repo(repo, auto, &host)?;
            let sub = format!("git:{rel}");
            let st_ev = if out.action == "refused" {
                refused_n += 1;
                State::Warn
            } else {
                State::Ok
            };
            emitter.emit(Event::Step { id: sub.clone(), state: st_ev, msg: out.msg.clone() });
            let peer_repo = format!("{}/{rel}", peer.home.trim_end_matches('/'));
            let exists = peer.run(&["test", "-d", &format!("{peer_repo}/.git")]).map(|o| o.status == 0).unwrap_or(false);
            if !exists {
                if out.action == "no-remote" {
                    continue;
                }
                let url = g(repo, &["remote", "get-url", "origin"])?.trim().to_string();
                let local_home = paths::home().to_string_lossy().into_owned();
                let url = if i_am_hub && url.starts_with(&local_home) && !peer.is_local() {
                    format!("{}:{}", crate::ssh::ALIAS, url.trim_start_matches(&local_home).trim_start_matches('/'))
                } else {
                    url
                };
                let parent = Path::new(&peer_repo).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                let branch = state(repo).map(|s| s.branch).unwrap_or_else(|_| "main".into());
                let script = format!("mkdir -p {} && git clone -q --branch {} {} {}", shell_quote(&parent), shell_quote(&branch), shell_quote(&url), shell_quote(&peer_repo));
                let o = peer.sh(&script)?;
                emitter.emit(Event::Step {
                    id: format!("{sub} @{}", peer.name),
                    state: if o.status == 0 { State::Ok } else { State::Warn },
                    msg: if o.status == 0 {
                        format!("cloned on {}", peer.name)
                    } else {
                        format!("clone on {} failed: {}", peer.name, o.stderr.trim())
                    },
                });
            } else {
                let mut args = vec!["git-sync-local"];
                if auto {
                    args.push("--autocommit");
                }
                args.push("--");
                args.push(rel.as_str());
                let outs: Vec<Outcome> = peer.twin_json(&args)?;
                if let Some(o) = outs.first() {
                    let s = if o.action == "refused" {
                        refused_n += 1;
                        State::Warn
                    } else {
                        State::Ok
                    };
                    emitter.emit(Event::Step { id: format!("{sub} @{}", peer.name), state: s, msg: o.msg.clone() });
                }
            }
            emitter.emit(Event::Progress { id: id.into(), done: i as u64 + 1, total, bytes: 0 });
        }
        emitter.emit(Event::Step {
            id: id.into(),
            state: if refused_n == 0 { State::Ok } else { State::Warn },
            msg: if refused_n == 0 { format!("{total} repos in sync") } else { format!("{refused_n} of {total} repos need attention") },
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn sh(dir: &Path, s: &str) {
        let o = run("sh", &["-c", s], Some(dir)).unwrap();
        assert_eq!(o.status, 0, "{s}: {}", o.stderr);
    }
    /// bare origin + two clones a, b
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        sh(d.path(), "git init -q --bare origin.git && git clone -q origin.git a 2>/dev/null && cd a && git config user.email t@t && git config user.name t && git commit -q --allow-empty -m init && git push -q -u origin HEAD 2>/dev/null");
        sh(d.path(), "git clone -q origin.git b 2>/dev/null && cd b && git config user.email t@t && git config user.name t");
        let a = d.path().join("a");
        let b = d.path().join("b");
        (d, a, b)
    }
    #[test]
    fn clean_repo_is_clean() {
        let (_d, a, _b) = fixture();
        assert_eq!(sync_repo(&a, true, "h").unwrap().action, "clean");
    }
    #[test]
    fn dirty_without_autocommit_is_refused() {
        let (_d, a, _b) = fixture();
        std::fs::write(a.join("f"), "x").unwrap();
        assert_eq!(sync_repo(&a, false, "h").unwrap().action, "refused");
    }
    #[test]
    fn dirty_with_autocommit_commits_and_pushes_then_other_side_fast_forwards() {
        let (_d, a, b) = fixture();
        std::fs::write(a.join("f"), "x").unwrap();
        assert_eq!(sync_repo(&a, true, "mac").unwrap().action, "pushed");
        assert_eq!(sync_repo(&b, true, "desktop").unwrap().action, "fast-forwarded");
        assert!(b.join("f").exists());
        assert!(g(&b, &["log", "-1", "--format=%s"]).unwrap().starts_with("twin: mac"));
    }
    #[test]
    fn diverged_without_conflict_rebases() {
        let (_d, a, b) = fixture();
        std::fs::write(a.join("a.txt"), "a").unwrap();
        assert_eq!(sync_repo(&a, true, "h").unwrap().action, "pushed");
        std::fs::write(b.join("b.txt"), "b").unwrap();
        assert_eq!(sync_repo(&b, true, "h").unwrap().action, "rebased-and-pushed");
        assert_eq!(sync_repo(&a, true, "h").unwrap().action, "fast-forwarded");
        assert!(a.join("b.txt").exists());
    }
    #[test]
    fn diverged_with_conflict_is_refused_and_repo_left_clean() {
        let (_d, a, b) = fixture();
        std::fs::write(a.join("same.txt"), "a").unwrap();
        sync_repo(&a, true, "h").unwrap();
        std::fs::write(b.join("same.txt"), "b").unwrap();
        let o = sync_repo(&b, true, "h").unwrap();
        assert_eq!(o.action, "refused");
        assert!(!state(&b).unwrap().in_progress);
    }
    #[test]
    fn unborn_repo_commits_then_reports() {
        let d = tempfile::tempdir().unwrap();
        sh(d.path(), "git init -q r && cd r && git config user.email t@t && git config user.name t");
        let r = d.path().join("r");
        assert_eq!(sync_repo(&r, true, "h").unwrap().action, "clean");
        std::fs::write(r.join("f"), "x").unwrap();
        assert_eq!(sync_repo(&r, true, "h").unwrap().action, "no-remote");
        assert!(g_ok(&r, &["rev-parse", "--verify", "-q", "HEAD"]));
    }
    #[test]
    fn no_remote_reports_no_remote() {
        let d = tempfile::tempdir().unwrap();
        sh(d.path(), "git init -q r && cd r && git config user.email t@t && git config user.name t && git commit -q --allow-empty -m init");
        assert_eq!(sync_repo(&d.path().join("r"), true, "h").unwrap().action, "no-remote");
    }
}
