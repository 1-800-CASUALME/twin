use anyhow::{bail, Result};
use std::path::PathBuf;

pub struct RunLock {
    path: PathBuf,
}

impl RunLock {
    pub fn acquire() -> Result<RunLock> {
        Self::acquire_at(crate::paths::state_dir().join("lock"))
    }
    pub fn acquire_at(path: PathBuf) -> Result<RunLock> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
                Ok(RunLock { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let pid = std::fs::read_to_string(&path).unwrap_or_default();
                bail!(
                    "another twin run is in progress (pid {}); remove {} if it is stale",
                    pid.trim(),
                    path.display()
                )
            }
            Err(e) => Err(e.into()),
        }
    }
}
impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_acquire_fails_until_drop() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("lock");
        let l1 = RunLock::acquire_at(p.clone()).unwrap();
        assert!(RunLock::acquire_at(p.clone()).is_err());
        drop(l1);
        assert!(RunLock::acquire_at(p).is_ok());
    }
}
