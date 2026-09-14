use crate::cmd::{self, Output};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

pub const ALIAS: &str = "twin-peer";
const BEGIN: &str = "# BEGIN twin";
const END: &str = "# END twin";

pub fn peer_block(addr: &str, user: &str, identity_file: &str) -> String {
    format!(
        "{BEGIN}\nHost {ALIAS}\n    HostName {addr}\n    User {user}\n    IdentityFile {identity_file}\n    IdentitiesOnly yes\n    StrictHostKeyChecking accept-new\n    ServerAliveInterval 15\n{END}\n"
    )
}

pub fn write_peer_host(addr: &str, user: &str, identity_file: &str) -> Result<()> {
    write_peer_host_to(&crate::paths::ssh_config(), addr, user, identity_file)
}

pub fn write_peer_host_to(cfg: &Path, addr: &str, user: &str, identity_file: &str) -> Result<()> {
    if let Some(d) = cfg.parent() {
        std::fs::create_dir_all(d)?;
    }
    let existing = std::fs::read_to_string(cfg).unwrap_or_default();
    let stripped = strip_block(&existing);
    let mut out = stripped.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&peer_block(addr, user, identity_file));
    std::fs::write(cfg, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cfg, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn strip_block(s: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in s.lines() {
        if line.trim() == BEGIN {
            skipping = true;
            continue;
        }
        if line.trim() == END {
            skipping = false;
            continue;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub fn authorize_key(pubkey_line: &str) -> Result<()> {
    authorize_key_in(&crate::paths::authorized_keys(), pubkey_line)
}

pub fn authorize_key_in(file: &Path, pubkey_line: &str) -> Result<()> {
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d)?;
    }
    let existing = std::fs::read_to_string(file).unwrap_or_default();
    let key = pubkey_line.trim();
    if existing.lines().any(|l| l.trim() == key) {
        return Ok(());
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(key);
    out.push('\n');
    std::fs::write(file, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn identity_file_string() -> PathBuf {
    crate::paths::identity_dir().join("id_ed25519")
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub struct Peer {
    pub home: String,
    pub user: String,
    pub name: String,
}

impl Peer {
    pub fn new(cfg: &Config) -> Result<Peer> {
        let p = cfg.peer.as_ref().context("not paired yet: run `twin pair` first")?;
        Ok(Peer { home: p.home.clone(), user: p.user.clone(), name: p.name.clone() })
    }

    fn ssh_base_args() -> Vec<String> {
        vec![
            "-F".into(),
            crate::paths::ssh_config().to_string_lossy().into_owned(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "ConnectTimeout=5".into(),
            ALIAS.into(),
            "--".into(),
        ]
    }

    /// True when TWIN_PEER_LOCAL is set: the "peer" is a second home directory on this
    /// machine and commands run locally with HOME swapped. Used by the integration test.
    pub fn is_local(&self) -> bool {
        std::env::var("TWIN_PEER_LOCAL").map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Run a shell command line on the peer through a login shell so PATH is the user's.
    pub fn sh(&self, script: &str) -> Result<Output> {
        let script = match std::env::var("TWIN_PEER_PATH_PREFIX") {
            Ok(p) if !p.is_empty() => format!("export PATH={}:\"$PATH\"; {}", shell_quote(&p), script),
            _ => script.to_string(),
        };
        if self.is_local() {
            let script = format!("export HOME={}; cd \"$HOME\"; {}", shell_quote(&self.home), script);
            return cmd::run("sh", &["-c", &script], None);
        }
        let mut a = Self::ssh_base_args();
        a.push("sh".into());
        a.push("-lc".into());
        a.push(shell_quote(&script));
        let refs: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
        cmd::run("ssh", &refs, None)
    }

    /// Run a plain argv on the peer (no shell quoting applied beyond ssh's own join).
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        let script = args.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
        self.sh(&script)
    }

    pub fn reachable(&self) -> bool {
        self.run(&["true"]).map(|o| o.status == 0).unwrap_or(false)
    }

    /// Run the peer's twin binary and parse its NDJSON stdout.
    pub fn twin_json<T: DeserializeOwned>(&self, twin_args: &[&str]) -> Result<Vec<T>> {
        let mut argv = vec!["twin"];
        argv.extend_from_slice(twin_args);
        let o = self.run(&argv)?;
        if o.status != 0 {
            bail!("peer twin {} failed: {}", twin_args.join(" "), o.stderr.trim());
        }
        let mut v = Vec::new();
        for line in o.stdout.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(t) = serde_json::from_str::<T>(line) {
                v.push(t);
            }
        }
        Ok(v)
    }

    fn rsync(&self, src: &str, dst: &str, files_from: &Path) -> Result<()> {
        if self.is_local() {
            let strip = |s: &str| s.trim_start_matches(&format!("{ALIAS}:")).to_string();
            cmd::run_ok("rsync", &["-a", "--files-from", files_from.to_str().unwrap(), &strip(src), &strip(dst)], None)?;
            return Ok(());
        }
        let ssh_cmd = format!("ssh -F {} -o BatchMode=yes", shell_quote(&crate::paths::ssh_config().to_string_lossy()));
        cmd::run_ok(
            "rsync",
            &["-a", "-e", &ssh_cmd, "--files-from", files_from.to_str().unwrap(), src, dst],
            None,
        )?;
        Ok(())
    }

    pub fn rsync_push(&self, local_dir: &Path, remote_dir: &str, files_from: &Path) -> Result<()> {
        let o = self.run(&["mkdir", "-p", remote_dir])?;
        if o.status != 0 {
            bail!("mkdir on {} failed: {}", self.name, o.stderr.trim());
        }
        let src = format!("{}/", local_dir.display());
        let dst = format!("{ALIAS}:{}/", remote_dir);
        self.rsync(&src, &dst, files_from)
    }

    pub fn rsync_pull(&self, remote_dir: &str, local_dir: &Path, files_from: &Path) -> Result<()> {
        std::fs::create_dir_all(local_dir)?;
        let src = format!("{ALIAS}:{}/", remote_dir);
        let dst = format!("{}/", local_dir.display());
        self.rsync(&src, &dst, files_from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peer_block_is_idempotent_and_preserves_other_hosts() {
        let d = tempfile::tempdir().unwrap();
        let cfg = d.path().join("config");
        std::fs::write(&cfg, "Host other\n    HostName 1.2.3.4\n").unwrap();
        write_peer_host_to(&cfg, "10.0.0.2", "asim", "/x/id").unwrap();
        write_peer_host_to(&cfg, "10.0.0.3", "asim", "/x/id").unwrap();
        let s = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(s.matches("Host twin-peer").count(), 1);
        assert!(s.contains("Host other"));
        assert!(s.contains("HostName 10.0.0.3"));
        assert!(!s.contains("10.0.0.2"));
    }
    #[test]
    fn authorize_key_appends_once() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("authorized_keys");
        authorize_key_in(&f, "ssh-ed25519 AAAA twin@mac").unwrap();
        authorize_key_in(&f, "ssh-ed25519 AAAA twin@mac").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap().lines().count(), 1);
    }
    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }
}
