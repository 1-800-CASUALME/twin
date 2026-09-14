use crate::cmd;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Identity {
    pub private: PathBuf,
    pub public: PathBuf,
    pub public_key: String,
}

/// Short stable fingerprint of this machine's Twin key (used to recognise a peer across
/// hostname and IP changes).
pub fn fingerprint(public_key: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(public_key.trim().as_bytes())[..6])
}

pub fn ensure() -> Result<Identity> {
    ensure_in(crate::paths::identity_dir())
}

pub fn ensure_in(dir: PathBuf) -> Result<Identity> {
    std::fs::create_dir_all(&dir)?;
    let private = dir.join("id_ed25519");
    let public = dir.join("id_ed25519.pub");
    if !private.exists() {
        let host = hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_else(|_| "twin".into());
        cmd::run_ok(
            "ssh-keygen",
            &["-q", "-t", "ed25519", "-N", "", "-C", &format!("twin@{host}"), "-f", private.to_str().unwrap()],
            None,
        )
        .context("ssh-keygen")?;
    }
    let public_key = std::fs::read_to_string(&public)?.trim().to_string();
    Ok(Identity { private, public, public_key })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creates_key_once() {
        let d = tempfile::tempdir().unwrap();
        let a = ensure_in(d.path().to_path_buf()).unwrap();
        let b = ensure_in(d.path().to_path_buf()).unwrap();
        assert!(a.public_key.starts_with("ssh-ed25519 "));
        assert_eq!(a.public_key, b.public_key);
    }
}
