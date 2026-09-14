//! Shell history through atuin, with the server self-hosted on the hub.
use super::Engine;
use crate::cmd::{run, run_ok, which};
use crate::config::Config;
use crate::event::{Emitter, Event, State};
use crate::paths;
use crate::ssh::{shell_quote, Peer};
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const PORT: u16 = 8888;
pub const HOSTED: &str = "https://api.atuin.sh";

/// Can this machine run `atuin server`? (Distro builds sometimes omit the server feature.)
pub fn server_capable() -> bool {
    run("atuin", &["server", "--help"], None).map(|o| o.status == 0).unwrap_or(false)
        || which("atuin-server")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub password: String,
    pub key: String,
}

fn account_file() -> std::path::PathBuf {
    paths::state_dir().join("atuin.json")
}

pub fn account() -> Option<Account> {
    std::fs::read_to_string(account_file()).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn random_password() -> String {
    use sha2::{Digest, Sha256};
    let seed = format!("{}{:?}", std::process::id(), std::time::SystemTime::now());
    hex::encode(&Sha256::digest(seed.as_bytes())[..16])
}

/// Server config + systemd user unit on the hub (Linux). Returns Ok(msg) or Err.
fn setup_server_here() -> Result<String> {
    let home = paths::home();
    let cfg_dir = home.join(".config/atuin");
    std::fs::create_dir_all(&cfg_dir)?;
    std::fs::create_dir_all(home.join(".local/share/atuin"))?;
    let server = cfg_dir.join("server.toml");
    if !server.exists() {
        std::fs::write(
            &server,
            format!(
                "host = \"0.0.0.0\"\nport = {PORT}\nopen_registration = true\ndb_uri = \"sqlite://{}/.local/share/atuin/server.db\"\n",
                home.display()
            ),
        )?;
    }
    if cfg!(target_os = "linux") {
        let unit_dir = home.join(".config/systemd/user");
        std::fs::create_dir_all(&unit_dir)?;
        let atuin = run("sh", &["-c", "command -v atuin"], None).map(|o| o.stdout.trim().to_string()).unwrap_or("atuin".into());
        std::fs::write(
            unit_dir.join("atuin-server.service"),
            format!("[Unit]\nDescription=atuin sync server (Twin)\nAfter=network.target\n\n[Service]\nExecStart={atuin} server start\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n"),
        )?;
        run_ok("systemctl", &["--user", "daemon-reload"], None)?;
        run_ok("systemctl", &["--user", "enable", "--now", "atuin-server.service"], None)?;
        Ok("atuin-server running as a systemd user service".into())
    } else {
        Ok("server config written (start with: atuin server start)".into())
    }
}

fn write_client_config(sync_address: &str) -> Result<()> {
    let cfg_dir = paths::home().join(".config/atuin");
    std::fs::create_dir_all(&cfg_dir)?;
    let p = cfg_dir.join("config.toml");
    let mut s = std::fs::read_to_string(&p).unwrap_or_default();
    let mut lines: Vec<String> = s.lines().filter(|l| !l.starts_with("sync_address") && !l.starts_with("auto_sync") && !l.starts_with("sync_frequency")).map(String::from).collect();
    lines.push(format!("sync_address = \"{sync_address}\""));
    lines.push("auto_sync = true".into());
    lines.push("sync_frequency = \"5m\"".into());
    s = lines.join("\n") + "\n";
    std::fs::write(&p, s)?;
    Ok(())
}

pub struct HistoryEngine;

impl Engine for HistoryEngine {
    fn id(&self) -> &'static str {
        "history"
    }
    fn sync(&self, cfg: &Config, peer: &Peer, _members: &[String], emitter: &dyn Emitter) -> Result<()> {
        let id = "history";
        emitter.emit(Event::Step { id: id.into(), state: State::Running, msg: "atuin".into() });
        if !which("atuin") {
            emitter.emit(Event::Step { id: id.into(), state: State::Warn, msg: "atuin is not installed here (Fix it on the Diagnose screen)".into() });
            return Ok(());
        }
        if !peer.sh("command -v atuin >/dev/null").map(|o| o.status == 0).unwrap_or(false) {
            emitter.emit(Event::Step { id: id.into(), state: State::Warn, msg: format!("atuin is not installed on {}", peer.name) });
            return Ok(());
        }
        let peer_cfg = cfg.peer.as_ref().unwrap();
        let i_am_hub = !peer_cfg.hub;
        // 1. server on the hub, if its atuin build can run one; otherwise the hosted, end-to-end encrypted sync
        let hub_capable = if i_am_hub { server_capable() } else { peer.sh("atuin server --help >/dev/null 2>&1 || command -v atuin-server >/dev/null").map(|o| o.status == 0).unwrap_or(false) };
        let (server_msg, server_state, self_hosted) = if !hub_capable {
            (format!("{} cannot run `atuin server`; using {HOSTED} (history stays end-to-end encrypted)", if i_am_hub { "this machine" } else { &peer.name }), State::Warn, false)
        } else if i_am_hub {
            (setup_server_here()?, State::Ok, true)
        } else {
            let o = peer.sh("twin atuin-server-setup")?;
            if o.status != 0 {
                (format!("server setup on {} failed: {}", peer.name, o.stderr.trim()), State::Warn, true)
            } else {
                (format!("server on {}", peer.name), State::Ok, true)
            }
        };
        emitter.emit(Event::Step { id: format!("{id}:server"), state: server_state, msg: server_msg });
        // 2. client config on both sides, pointing at the hub (or the hosted server)
        let (here, there) = if !self_hosted {
            (HOSTED.to_string(), HOSTED.to_string())
        } else if i_am_hub {
            (format!("http://127.0.0.1:{PORT}"), "peer".to_string()) // the peer resolves "peer" to our address from its own config
        } else {
            (format!("http://{}:{PORT}", peer_cfg.addr), format!("http://127.0.0.1:{PORT}"))
        };
        write_client_config(&here)?;
        let _ = peer.sh(&format!("twin atuin-client-config -- {}", shell_quote(&there)));
        if std::env::var("TWIN_ATUIN_DRY").is_ok() {
            emitter.emit(Event::Step { id: id.into(), state: State::Ok, msg: format!("configs written (dry): here {here}, there {there}") });
            return Ok(());
        }
        // 3. one account, registered on the hub, key shared
        let acct = match account() {
            Some(a) => a,
            None => {
                let username = std::env::var("USER").unwrap_or("twin".into()).to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "");
                let password = random_password();
                let email = format!("{username}@twin.local");
                let reg = run("atuin", &["register", "-u", &username, "-p", &password, "-e", &email], None)?;
                if reg.status != 0 && !reg.stderr.contains("already") {
                    let login = run("atuin", &["login", "-u", &username, "-p", &password], None)?;
                    if login.status != 0 {
                        emitter.emit(Event::Step { id: id.into(), state: State::Warn, msg: format!("atuin register failed: {}", reg.stderr.trim()) });
                        return Ok(());
                    }
                }
                let key = run_ok("atuin", &["key"], None)?.trim().to_string();
                let a = Account { username, password, key };
                std::fs::create_dir_all(paths::state_dir())?;
                std::fs::write(account_file(), serde_json::to_string_pretty(&a)?)?;
                a
            }
        };
        let json = serde_json::to_string(&acct)?;
        let peer_login = format!("printf %s {} | twin atuin-login", shell_quote(&json));
        let o = peer.sh(&peer_login)?;
        emitter.emit(Event::Step { id: format!("{id}:account"), state: if o.status == 0 { State::Ok } else { State::Warn }, msg: if o.status == 0 { format!("{} logged in on {}", acct.username, peer.name) } else { format!("login on {} failed: {}", peer.name, o.stderr.trim()) } });
        // 4. import existing history once, then sync
        let marker = paths::state_dir().join("atuin-imported");
        if !marker.exists() {
            let _ = run("atuin", &["import", "auto"], None);
            let _ = std::fs::write(&marker, "");
        }
        let here = run("atuin", &["sync"], None)?;
        let there = peer.sh("atuin sync")?;
        emitter.emit(Event::Step {
            id: id.into(),
            state: if here.status == 0 && there.status == 0 { State::Ok } else { State::Warn },
            msg: format!(
                "synced here{}{}",
                if here.status == 0 { "" } else { " (failed)" },
                if there.status == 0 { format!(" and on {}", peer.name) } else { format!(", failed on {}: {}", peer.name, there.stderr.trim()) }
            ),
        });
        Ok(())
    }
}

/// Peer-side helper: log in with a shared account (JSON on stdin) and import history once.
pub fn login_from_json(json: &str) -> Result<()> {
    let a: Account = serde_json::from_str(json)?;
    std::fs::create_dir_all(paths::state_dir())?;
    std::fs::write(account_file(), serde_json::to_string_pretty(&a)?)?;
    let status = run("atuin", &["status"], None)?;
    if !status.stdout.contains(&a.username) {
        run_ok("atuin", &["login", "-u", &a.username, "-p", &a.password, "-k", &a.key], None)?;
    }
    let marker = paths::state_dir().join("atuin-imported");
    if !marker.exists() {
        let _ = run("atuin", &["import", "auto"], None);
        let _ = std::fs::write(&marker, "");
    }
    Ok(())
}

pub fn server_setup() -> Result<String> {
    setup_server_here()
}
pub fn client_config(sync_address: &str) -> Result<()> {
    let addr = if sync_address == "peer" {
        let cfg = Config::load()?;
        let a = cfg.peer.as_ref().map(|p| p.addr.clone()).unwrap_or("127.0.0.1".into());
        format!("http://{a}:{PORT}")
    } else {
        sync_address.to_string()
    };
    write_client_config(&addr)
}
