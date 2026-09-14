use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{BufRead, Read};
use std::time::Duration;
use twin_core::event::{Emitter, Event, State, StdoutEmitter};
use twin_core::{cmd, config::Config, diagnose, discover, engines, inventory, lock::RunLock, log::RunLog, pair, ssh::Peer};

#[derive(Parser)]
#[command(name = "twin", version, about = "Keep two workstations in sync")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find the other machine on the LAN
    Discover {
        #[arg(long, default_value_t = 4)]
        timeout: u64,
    },
    /// Advertise on the LAN and accept pairing
    Daemon {
        #[arg(long, default_value_t = 7423)]
        port: u16,
        #[arg(long)]
        accept: bool,
    },
    /// Pair with a discovered machine
    Pair {
        #[arg(long)]
        instance: Option<String>,
        #[arg(long)]
        addr: Option<String>,
        #[arg(long, default_value_t = 7423)]
        port: u16,
        #[arg(long)]
        yes: bool,
    },
    /// Check both machines
    Diagnose {
        #[arg(long)]
        local: bool,
    },
    /// Fix one failed check
    Fix { id: String },
    /// List what can be synced
    Inventory {
        #[arg(long)]
        local: bool,
    },
    /// Sync selected items (claude, git) or --all
    Sync {
        items: Vec<String>,
        #[arg(long)]
        all: bool,
        /// Restrict to members, as item:member (repeatable)
        #[arg(long)]
        member: Vec<String>,
    },
    /// Show pairing status
    Status,
    /// Save which items and members to sync by default
    Select {
        items: Vec<String>,
        /// item:member keys to restrict to (repeatable)
        #[arg(long)]
        member: Vec<String>,
        /// home-relative repo paths that may be auto-committed (repeatable)
        #[arg(long)]
        autocommit: Vec<String>,
    },
    #[command(hide = true)]
    ClaudeFiles {
        #[arg(allow_hyphen_values = true)]
        slug: String,
    },
    #[command(hide = true)]
    ClaudePrefixHash {
        #[arg(allow_hyphen_values = true)]
        slug: String,
    },
    #[command(hide = true)]
    ConflictCopy {
        #[arg(allow_hyphen_values = true)]
        path: String,
        host: String,
    },
    #[command(hide = true)]
    GitSyncLocal {
        #[arg(allow_hyphen_values = true)]
        rel: String,
        #[arg(long)]
        autocommit: bool,
    },
}

fn confirm_stdin(code: &str) -> bool {
    eprintln!("Pairing code: {code}\nDoes the other machine show the same code? [y/N] ");
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    matches!(s.trim().to_lowercase().as_str(), "y" | "yes")
}

fn main() {
    let em = StdoutEmitter;
    if let Err(e) = real_main(&em) {
        em.emit(Event::Error { msg: format!("{e:#}") });
        em.emit(Event::Done { ok: false });
        std::process::exit(1);
    }
}

fn real_main(em: &dyn Emitter) -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Discover { timeout } => {
            discover::browse(Duration::from_secs(timeout), em)?;
            em.emit(Event::Done { ok: true });
        }
        Cmd::Daemon { port, accept } => {
            let info = discover::local_info();
            let _adv = discover::advertise(port, &info)?;
            em.emit(Event::Step { id: "daemon".into(), state: State::Running, msg: format!("advertising {} on port {port}", info.host) });
            let f: Box<dyn Fn(&str) -> bool + Send + Sync> = if accept { Box::new(|_| true) } else { Box::new(confirm_stdin) };
            pair::serve(port, f, em)?;
        }
        Cmd::Pair { instance, addr, port, yes } => {
            let found = match (instance, addr) {
                (_, Some(a)) => discover::Found {
                    name: a.clone(),
                    host: a.clone(),
                    os: String::new(),
                    user: String::new(),
                    home: String::new(),
                    addr: a,
                    port,
                    instance: String::new(),
                },
                (inst, None) => {
                    let peers = discover::browse(Duration::from_secs(4), em)?;
                    let p = match inst {
                        Some(i) => peers.into_iter().find(|p| p.instance == i),
                        None => peers.into_iter().next(),
                    };
                    p.ok_or_else(|| anyhow::anyhow!("no twin daemon found on the LAN; run `twin daemon` on the other machine"))?
                }
            };
            let f: Box<dyn Fn(&str) -> bool> = if yes { Box::new(|_| true) } else { Box::new(confirm_stdin) };
            pair::connect(&found, f, em)?;
            em.emit(Event::Done { ok: true });
        }
        Cmd::Diagnose { local } => {
            if local {
                diagnose::run_local(em);
            } else {
                cmd::set_log(RunLog::start()?);
                diagnose::run_all(&Config::load()?, em)?;
            }
            em.emit(Event::Done { ok: true });
        }
        Cmd::Fix { id } => {
            cmd::set_log(RunLog::start()?);
            diagnose::fix(&id, em)?;
            em.emit(Event::Done { ok: true });
        }
        Cmd::Inventory { local } => {
            let cfg = Config::load()?;
            if local {
                for i in inventory::local(&cfg) {
                    em.emit(Event::Item(i));
                }
            } else {
                inventory::merged(&cfg, em)?;
            }
            em.emit(Event::Done { ok: true });
        }
        Cmd::Sync { items, all, member } => {
            let _lock = RunLock::acquire()?;
            cmd::set_log(RunLog::start()?);
            let cfg = Config::load()?;
            let peer = Peer::new(&cfg)?;
            if !peer.reachable() {
                anyhow::bail!("peer {} unreachable", peer.name);
            }
            let ids: Vec<String> = if all || items.is_empty() {
                if cfg.selection.is_empty() {
                    engines::all().iter().map(|e| e.id().to_string()).collect()
                } else {
                    cfg.selection.clone()
                }
            } else {
                items
            };
            let mut ok = true;
            for id in ids {
                let Some(e) = engines::by_id(&id) else {
                    em.emit(Event::Step { id: id.clone(), state: State::Skipped, msg: "unknown item".into() });
                    continue;
                };
                let member_src = if member.is_empty() { &cfg.members } else { &member };
                let members: Vec<String> = member_src
                    .iter()
                    .filter_map(|m| m.strip_prefix(&format!("{id}:")).map(|s| s.to_string()))
                    .collect();
                if let Err(err) = e.sync(&cfg, &peer, &members, em) {
                    ok = false;
                    em.emit(Event::Step { id: id.clone(), state: State::Fail, msg: format!("{err:#}") });
                }
            }
            em.emit(Event::Done { ok });
        }
        Cmd::Status => {
            let cfg = Config::load()?;
            match &cfg.peer {
                Some(p) => {
                    let reach = Peer::new(&cfg)?.reachable();
                    em.emit(Event::Step {
                        id: "peer".into(),
                        state: if reach { State::Ok } else { State::Warn },
                        msg: format!("{} ({}) {}", p.name, p.addr, if reach { "reachable" } else { "unreachable" }),
                    });
                }
                None => em.emit(Event::Step { id: "peer".into(), state: State::Fail, msg: "not paired".into() }),
            }
            em.emit(Event::Done { ok: true });
        }
        Cmd::Select { items, member, autocommit } => {
            let mut cfg = Config::load()?;
            cfg.selection = items;
            cfg.members = member;
            cfg.git_autocommit = autocommit;
            cfg.save()?;
            em.emit(Event::Step { id: "select".into(), state: State::Ok, msg: format!("{} items saved", cfg.selection.len()) });
            em.emit(Event::Done { ok: true });
        }
        Cmd::ClaudeFiles { slug } => {
            let root = twin_core::paths::claude_projects_dir().join(slug);
            if root.is_dir() {
                for f in engines::claude::list_files(&root) {
                    println!("{}", serde_json::to_string(&f)?);
                }
            }
        }
        Cmd::ClaudePrefixHash { slug } => {
            let root = twin_core::paths::claude_projects_dir().join(slug);
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            for line in input.lines() {
                let mut it = line.splitn(2, '\t');
                let (Some(rel), Some(n)) = (it.next(), it.next().and_then(|n| n.trim().parse::<u64>().ok())) else { continue };
                if let Ok(h) = engines::claude::prefix_hash(&root.join(rel), n) {
                    println!("{rel}\t{h}");
                }
            }
        }
        Cmd::ConflictCopy { path, host } => {
            let new = engines::claude::conflict_copy(std::path::Path::new(&path), &host)?;
            println!("{new}");
        }
        Cmd::GitSyncLocal { rel, autocommit } => {
            let repo = twin_core::paths::home().join(&rel);
            let host = discover::local_info().host;
            let out = engines::git::sync_repo(&repo, autocommit, &host)?;
            println!("{}", serde_json::to_string(&out)?);
        }
    }
    Ok(())
}
