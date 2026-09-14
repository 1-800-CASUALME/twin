use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct RunLog {
    file: Mutex<std::fs::File>,
    pub path: PathBuf,
}

impl RunLog {
    pub fn start() -> Result<RunLog> {
        let dir = crate::paths::log_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.log", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        let file = std::fs::File::create(&path)?;
        Ok(RunLog { file: Mutex::new(file), path })
    }
    pub fn line(&self, s: &str) {
        if let Ok(mut f) = self.file.lock() {
            let _ = writeln!(f, "{} {}", chrono::Local::now().format("%H:%M:%S"), s);
        }
    }
}
