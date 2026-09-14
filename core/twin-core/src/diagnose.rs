use crate::cmd::{self, which};
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckResult {
    pub id: String,
    pub name: String,
    pub side: String,
    pub state: State,
    pub msg: String,
    pub fixable: bool,
}

pub struct CheckDef {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub brew: Option<&'static str>,
    pub pacman: Option<&'static str>,
}

pub const CHECKS: &[CheckDef] = &[
    CheckDef { id: "ssh", name: "SSH", icon: "network", brew: None, pacman: Some("openssh") },
    CheckDef { id: "rsync", name: "rsync", icon: "arrow.left.arrow.right", brew: Some("rsync"), pacman: Some("rsync") },
    CheckDef { id: "git", name: "Git", icon: "arrow.triangle.branch", brew: Some("git"), pacman: Some("git") },
    CheckDef { id: "tmux", name: "tmux", icon: "terminal", brew: Some("tmux"), pacman: Some("tmux") },
    CheckDef { id: "et", name: "Eternal Terminal", icon: "bolt.horizontal", brew: Some("MisterTea/et/et"), pacman: Some("eternalterminal") },
    CheckDef { id: "atuin", name: "atuin", icon: "clock.arrow.circlepath", brew: Some("atuin"), pacman: Some("atuin") },
    CheckDef { id: "chezmoi", name: "chezmoi", icon: "doc.text", brew: Some("chezmoi"), pacman: Some("chezmoi") },
    CheckDef { id: "tmux-resurrect", name: "tmux layouts", icon: "rectangle.split.2x2", brew: None, pacman: None },
    CheckDef { id: "atuin-server", name: "atuin server", icon: "server.rack", brew: None, pacman: None },
    CheckDef { id: "claude", name: "Claude Code", icon: "sparkles", brew: None, pacman: None },
    CheckDef { id: "disk", name: "Disk space", icon: "internaldrive", brew: None, pacman: None },
    CheckDef { id: "conflicts", name: "Conflicts", icon: "exclamationmark.triangle", brew: None, pacman: None },
];

fn def(id: &str) -> Option<&'static CheckDef> {
    CHECKS.iter().find(|c| c.id == id)
}

/// Tools without which nothing can sync. Everything else is a warning, not a blocker.
const REQUIRED: &[&str] = &["ssh", "rsync", "git"];

fn tool(id: &str, side: &str, ok: bool, version: String) -> CheckResult {
    let d = def(id).unwrap();
    CheckResult {
        id: id.into(),
        name: d.name.into(),
        side: side.into(),
        state: if ok { State::Ok } else if REQUIRED.contains(&id) { State::Fail } else { State::Warn },
        msg: if ok { version } else { "not installed".into() },
        fixable: !ok && (d.brew.is_some() || d.pacman.is_some()),
    }
}

fn version_of(program: &str, flag: &str) -> String {
    cmd::run(program, &[flag], None)
        .map(|o| o.stdout.lines().next().unwrap_or("").trim().to_string())
        .unwrap_or_default()
}

fn emit_push(out: &mut Vec<CheckResult>, emitter: &dyn Emitter, r: CheckResult) {
    emitter.emit(Event::Check(r.clone()));
    out.push(r);
}

pub fn run_local(emitter: &dyn Emitter) -> Vec<CheckResult> {
    let side = "local";
    let mut out = Vec::new();
    let et_bin = if cfg!(target_os = "macos") { "et" } else { "etserver" };
    let tools: &[(&str, &str, &str)] = &[
        ("rsync", "rsync", "--version"),
        ("git", "git", "--version"),
        ("tmux", "tmux", "-V"),
        ("et", et_bin, "--version"),
        ("atuin", "atuin", "--version"),
        ("chezmoi", "chezmoi", "--version"),
    ];
    for (id, bin, flag) in tools {
        let ok = which(bin);
        let r = tool(id, side, ok, if ok { version_of(bin, flag) } else { String::new() });
        emit_push(&mut out, emitter, r);
    }
    let sshd_ok = which("sshd") || std::path::Path::new("/usr/sbin/sshd").exists();
    emit_push(&mut out, emitter, CheckResult {
        id: "ssh".into(),
        name: "SSH".into(),
        side: side.into(),
        state: if sshd_ok { State::Ok } else { State::Fail },
        msg: if sshd_ok { "sshd available".into() } else { "sshd missing".into() },
        fixable: !sshd_ok,
    });
    let rs = crate::engines::terminal::resurrect_dir().is_some();
    emit_push(&mut out, emitter, CheckResult {
        id: "tmux-resurrect".into(),
        name: "tmux layouts".into(),
        side: side.into(),
        state: if rs { State::Ok } else { State::Warn },
        msg: if rs { "tmux-resurrect saves found".into() } else { "tmux-resurrect not set up (optional)".into() },
        fixable: false,
    });
    if cfg!(target_os = "linux") {
        let active = cmd::run("systemctl", &["--user", "is-active", "atuin-server.service"], None).map(|o| o.status == 0).unwrap_or(false);
        emit_push(&mut out, emitter, CheckResult {
            id: "atuin-server".into(),
            name: "atuin server".into(),
            side: side.into(),
            state: if active { State::Ok } else { State::Warn },
            msg: if active { "running".into() } else { "not running yet (History sync sets it up)".into() },
            fixable: false,
        });
    }
    let cl = which("claude") && crate::paths::claude_projects_dir().is_dir();
    emit_push(&mut out, emitter, CheckResult {
        id: "claude".into(),
        name: "Claude Code".into(),
        side: side.into(),
        state: if cl { State::Ok } else { State::Warn },
        msg: if cl { version_of("claude", "--version") } else { "claude not found or no ~/.claude/projects".into() },
        fixable: false,
    });
    let free = free_bytes(&crate::paths::home());
    emit_push(&mut out, emitter, CheckResult {
        id: "disk".into(),
        name: "Disk space".into(),
        side: side.into(),
        state: if free > 5 * 1024 * 1024 * 1024 { State::Ok } else { State::Warn },
        msg: format!("{} free", human(free)),
        fixable: false,
    });
    let n = count_conflicts();
    emit_push(&mut out, emitter, CheckResult {
        id: "conflicts".into(),
        name: "Conflicts".into(),
        side: side.into(),
        state: if n == 0 { State::Ok } else { State::Warn },
        msg: if n == 0 { "none".into() } else { format!("{n} .twin-conflict files") },
        fixable: false,
    });
    out
}

pub fn run_all(cfg: &Config, emitter: &dyn Emitter) -> Result<Vec<CheckResult>> {
    let mut all = run_local(emitter);
    match crate::ssh::Peer::new(cfg) {
        Ok(peer) => {
            let reach = peer.reachable();
            emit_push(&mut all, emitter, CheckResult {
                id: "ssh".into(),
                name: "SSH".into(),
                side: "pair".into(),
                state: if reach { State::Ok } else { State::Fail },
                msg: if reach { format!("connected to {}", peer.name) } else { "peer unreachable".into() },
                fixable: !reach,
            });
            if reach {
                match peer.twin_json::<Event>(&["diagnose", "--local"]) {
                    Ok(evs) => {
                        for ev in evs {
                            if let Event::Check(mut c) = ev {
                                c.side = "peer".into();
                                emit_push(&mut all, emitter, c);
                            }
                        }
                    }
                    Err(e) => emit_push(&mut all, emitter, CheckResult {
                        id: "twin".into(),
                        name: "Twin on peer".into(),
                        side: "peer".into(),
                        state: State::Fail,
                        msg: e.to_string(),
                        fixable: false,
                    }),
                }
            }
        }
        Err(_) => emit_push(&mut all, emitter, CheckResult {
            id: "ssh".into(),
            name: "SSH".into(),
            side: "pair".into(),
            state: State::Fail,
            msg: "not paired".into(),
            fixable: false,
        }),
    }
    Ok(all)
}

pub fn fix(id: &str, emitter: &dyn Emitter) -> Result<()> {
    let d = def(id).ok_or_else(|| anyhow::anyhow!("unknown check {id}"))?;
    emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: format!("installing {}", d.name) });
    let res: Result<()> = if cfg!(target_os = "macos") {
        match d.brew {
            Some(pkg) => cmd::run_ok("brew", &["install", pkg], None).map(|_| ()),
            None => bail!("no fix for {id} on macOS"),
        }
    } else {
        match d.pacman {
            Some(pkg) => cmd::run_ok("sudo", &["pacman", "-S", "--noconfirm", "--needed", pkg], None).map(|_| ()),
            None => bail!("no fix for {id} on Linux"),
        }
    };
    match res {
        Ok(()) => {
            emitter.emit(Event::Step { id: id.into(), state: State::Ok, msg: "installed".into() });
            Ok(())
        }
        Err(e) => {
            emitter.emit(Event::Step { id: id.into(), state: State::Fail, msg: e.to_string() });
            Err(e)
        }
    }
}

fn free_bytes(p: &std::path::Path) -> u64 {
    cmd::run("df", &["-k", p.to_str().unwrap_or("/")], None)
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.split_whitespace().nth(3).and_then(|k| k.parse::<u64>().ok()))
        })
        .map(|k| k * 1024)
        .unwrap_or(0)
}

fn count_conflicts() -> usize {
    let mut n = 0;
    for root in [crate::paths::claude_projects_dir()] {
        if !root.is_dir() {
            continue;
        }
        for e in walkdir::WalkDir::new(root).into_iter().flatten() {
            if e.file_name().to_string_lossy().contains(".twin-conflict-") {
                n += 1;
            }
        }
    }
    n
}

pub fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(1_600_000_000), "1.5 GB");
    }
    #[test]
    fn local_run_reports_every_phase1_check() {
        let r = run_local(&crate::event::NullEmitter);
        for id in ["rsync", "git", "tmux", "ssh", "claude", "disk", "conflicts"] {
            assert!(r.iter().any(|c| c.id == id), "missing {id}");
        }
    }
}
