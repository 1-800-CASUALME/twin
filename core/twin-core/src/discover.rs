use crate::event::{Emitter, Event};
use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub const SERVICE: &str = "_twin._tcp.local.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalInfo {
    pub name: String,
    pub host: String,
    pub os: String,
    pub user: String,
    pub home: String,
    pub instance: String,
}

pub fn local_info() -> LocalInfo {
    let host = hostname::get().map(|h| h.to_string_lossy().into_owned()).unwrap_or_else(|_| "unknown".into());
    let host = host.trim_end_matches(".local").to_string();
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let home = crate::paths::home().to_string_lossy().into_owned();
    let instance = format!("{}-{}", host, std::process::id());
    LocalInfo { name: host.clone(), host, os: std::env::consts::OS.to_string(), user, home, instance }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Found {
    pub name: String,
    pub host: String,
    pub os: String,
    pub user: String,
    pub home: String,
    pub addr: String,
    pub port: u16,
    pub instance: String,
}

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}
impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

pub fn advertise(port: u16, info: &LocalInfo) -> Result<Advertiser> {
    let daemon = ServiceDaemon::new()?;
    let props: HashMap<String, String> = [
        ("host", info.host.as_str()),
        ("os", info.os.as_str()),
        ("user", info.user.as_str()),
        ("home", info.home.as_str()),
        ("instance", info.instance.as_str()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let hostname = format!("{}.local.", info.host);
    let svc = ServiceInfo::new(SERVICE, &info.instance, &hostname, "", port, props)?.enable_addr_auto();
    let fullname = svc.get_fullname().to_string();
    daemon.register(svc)?;
    Ok(Advertiser { daemon, fullname })
}

pub fn browse(timeout: Duration, emitter: &dyn Emitter) -> Result<Vec<Found>> {
    let me = local_info();
    let daemon = ServiceDaemon::new()?;
    let rx = daemon.browse(SERVICE)?;
    let deadline = std::time::Instant::now() + timeout;
    let mut found: Vec<Found> = Vec::new();
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let get = |k: &str| info.get_property_val_str(k).unwrap_or("").to_string();
                if get("instance") == me.instance {
                    continue;
                }
                let addrs = info.get_addresses();
                let addr = addrs.iter().find(|a| a.is_ipv4()).or_else(|| addrs.iter().next()).map(|a| a.to_string());
                let Some(addr) = addr else { continue };
                if found.iter().any(|f| f.instance == get("instance")) {
                    continue;
                }
                let f = Found {
                    name: get("host"),
                    host: get("host"),
                    os: get("os"),
                    user: get("user"),
                    home: get("home"),
                    addr,
                    port: info.get_port(),
                    instance: get("instance"),
                };
                emitter.emit(Event::Peer {
                    name: f.name.clone(),
                    host: f.host.clone(),
                    os: f.os.clone(),
                    addr: f.addr.clone(),
                    port: f.port,
                    instance: f.instance.clone(),
                });
                found.push(f);
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_info_has_home_and_os() {
        let i = local_info();
        assert!(!i.home.is_empty());
        assert!(i.os == "macos" || i.os == "linux");
    }
    /// Loopback advertise + browse. Ignored by default because it needs multicast.
    #[test]
    #[ignore]
    fn advertise_then_browse_finds_other_instance() {
        let mut info = local_info();
        info.instance = "test-instance-1".into();
        let _adv = advertise(7999, &info).unwrap();
        let found = browse(Duration::from_secs(3), &crate::event::NullEmitter).unwrap();
        assert!(found.iter().any(|f| f.instance == "test-instance-1"));
    }
}
