use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    dirs::home_dir().expect("home dir")
}
pub fn twin_dir() -> PathBuf {
    home().join(".twin")
}
pub fn config_file() -> PathBuf {
    twin_dir().join("config.toml")
}
pub fn identity_dir() -> PathBuf {
    twin_dir().join("identity")
}
pub fn state_dir() -> PathBuf {
    twin_dir().join("state")
}
pub fn log_dir() -> PathBuf {
    twin_dir().join("log")
}
pub fn claude_projects_dir() -> PathBuf {
    home().join(".claude").join("projects")
}
pub fn ssh_dir() -> PathBuf {
    home().join(".ssh")
}
pub fn ssh_config() -> PathBuf {
    ssh_dir().join("config")
}
pub fn authorized_keys() -> PathBuf {
    ssh_dir().join("authorized_keys")
}

/// Path relative to home, forward slashes, no leading slash. None if outside home.
pub fn home_relative(p: &Path) -> Option<String> {
    p.strip_prefix(home()).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
}
