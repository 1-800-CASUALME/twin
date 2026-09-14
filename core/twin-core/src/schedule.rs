//! Background sync every 15 minutes: launchd agent on macOS, systemd user timer on Linux.
use crate::cmd::{run, run_ok};
use crate::config::Config;
use crate::paths;
use anyhow::Result;

const LABEL: &str = "com.asim.twin.sync";

fn twin_bin() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or("twin".into())
}

pub fn on() -> Result<String> {
    std::fs::create_dir_all(paths::log_dir())?;
    let log = paths::log_dir().join("schedule.log");
    let msg = if cfg!(target_os = "macos") {
        let dir = paths::home().join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir)?;
        let plist = dir.join(format!("{LABEL}.plist"));
        std::fs::write(
            &plist,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{bin}</string><string>sync</string><string>--all</string></array>
  <key>StartInterval</key><integer>900</integer>
  <key>RunAtLoad</key><false/>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:{home}/.local/bin</string></dict>
</dict></plist>
"#,
                bin = twin_bin(),
                log = log.display(),
                home = paths::home().display()
            ),
        )?;
        let uid = run("id", &["-u"], None)?.stdout.trim().to_string();
        let _ = run("launchctl", &["bootout", &format!("gui/{uid}/{LABEL}")], None);
        run_ok("launchctl", &["bootstrap", &format!("gui/{uid}"), plist.to_str().unwrap()], None)?;
        "launchd agent installed, every 15 minutes".to_string()
    } else {
        let dir = paths::home().join(".config/systemd/user");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("twin-sync.service"),
            format!("[Unit]\nDescription=Twin sync\n\n[Service]\nType=oneshot\nExecStart={} sync --all\nStandardOutput=append:{}\nStandardError=append:{}\n", twin_bin(), log.display(), log.display()),
        )?;
        std::fs::write(
            dir.join("twin-sync.timer"),
            "[Unit]\nDescription=Twin sync every 15 minutes\n\n[Timer]\nOnBootSec=5min\nOnUnitActiveSec=15min\n\n[Install]\nWantedBy=timers.target\n",
        )?;
        run_ok("systemctl", &["--user", "daemon-reload"], None)?;
        run_ok("systemctl", &["--user", "enable", "--now", "twin-sync.timer"], None)?;
        "systemd user timer installed, every 15 minutes".to_string()
    };
    let mut cfg = Config::load()?;
    cfg.schedule = true;
    cfg.save()?;
    Ok(msg)
}

pub fn off() -> Result<String> {
    if cfg!(target_os = "macos") {
        let uid = run("id", &["-u"], None)?.stdout.trim().to_string();
        let _ = run("launchctl", &["bootout", &format!("gui/{uid}/{LABEL}")], None);
        let _ = std::fs::remove_file(paths::home().join(format!("Library/LaunchAgents/{LABEL}.plist")));
    } else {
        let _ = run("systemctl", &["--user", "disable", "--now", "twin-sync.timer"], None);
    }
    let mut cfg = Config::load()?;
    cfg.schedule = false;
    cfg.save()?;
    Ok("background sync off".into())
}

pub fn is_on() -> bool {
    Config::load().map(|c| c.schedule).unwrap_or(false)
}
