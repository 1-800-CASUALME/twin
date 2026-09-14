use crate::log::RunLog;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

static LOG: OnceLock<RunLog> = OnceLock::new();
pub fn set_log(l: RunLog) {
    let _ = LOG.set(l);
}
pub fn log(s: &str) {
    if let Some(l) = LOG.get() {
        l.line(s);
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<Output> {
    log(&format!(
        "$ {} {}{}",
        program,
        args.join(" "),
        cwd.map(|c| format!("  (cwd {})", c.display())).unwrap_or_default()
    ));
    let mut c = Command::new(program);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().with_context(|| format!("failed to start {program}"))?;
    let o = Output {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    if o.status != 0 {
        log(&format!("  exit {}: {}", o.status, o.stderr.trim()));
    }
    Ok(o)
}

pub fn run_ok(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let o = run(program, args, cwd)?;
    if o.status != 0 {
        bail!("{} {} failed ({}): {}", program, args.join(" "), o.status, o.stderr.trim());
    }
    Ok(o.stdout)
}

pub fn which(program: &str) -> bool {
    run("sh", &["-c", &format!("command -v {} >/dev/null 2>&1", program)], None)
        .map(|o| o.status == 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_captures_stdout() {
        assert_eq!(run_ok("echo", &["hi"], None).unwrap().trim(), "hi");
    }
    #[test]
    fn run_ok_fails_on_nonzero() {
        assert!(run_ok("sh", &["-c", "exit 3"], None).is_err());
    }
    #[test]
    fn which_finds_sh() {
        assert!(which("sh"));
        assert!(!which("definitely-not-a-real-binary-xyz"));
    }
}
