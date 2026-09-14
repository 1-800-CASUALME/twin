use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PeerConfig {
    pub name: String,
    pub host: String,
    pub os: String,
    pub user: String,
    pub home: String,
    pub addr: String,
    /// true if the peer is the hub that hosts bare repos and atuin-server
    pub hub: bool,
    /// fingerprint of the peer's Twin key; stable across hostname and IP changes
    #[serde(default)]
    pub fp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    pub peer: Option<PeerConfig>,
    #[serde(default)]
    pub selection: Vec<String>,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub git_autocommit: Vec<String>,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub schedule: bool,
}

impl Config {
    pub fn load() -> Result<Config> {
        Self::load_from(&crate::paths::config_file())
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&crate::paths::config_file())
    }

    pub fn load_from(p: &Path) -> Result<Config> {
        if !p.exists() {
            return Ok(Config::default());
        }
        let s = std::fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
        toml::from_str(&s).with_context(|| format!("parse {}", p.display()))
    }
    pub fn save_to(&self, p: &Path) -> Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, toml::to_string_pretty(self)?)?;
        Ok(())
    }
    /// Home-relative path of a peer counterpart, joined onto the peer's home.
    pub fn peer_path(&self, home_relative: &str) -> Option<String> {
        self.peer.as_ref().map(|p| format!("{}/{}", p.home.trim_end_matches('/'), home_relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let mut c = Config::default();
        c.peer = Some(PeerConfig {
            name: "desktop".into(),
            host: "desktop".into(),
            os: "linux".into(),
            user: "asim".into(),
            home: "/home/asim".into(),
            addr: "192.168.1.5".into(),
            hub: true,
            fp: "abc".into(),
        });
        c.selection = vec!["claude".into(), "git".into()];
        c.save_to(&p).unwrap();
        assert_eq!(Config::load_from(&p).unwrap(), c);
    }
    #[test]
    fn missing_file_is_default() {
        assert_eq!(Config::load_from(Path::new("/nonexistent/x.toml")).unwrap(), Config::default());
    }
    #[test]
    fn peer_path_joins_home() {
        let mut c = Config::default();
        c.peer = Some(PeerConfig { home: "/home/asim/".into(), ..Default::default() });
        assert_eq!(c.peer_path("wagt").unwrap(), "/home/asim/wagt");
    }
}
