use crate::config::{Config, PeerConfig};
use crate::discover::{local_info, Found, LocalInfo};
use crate::event::{Emitter, Event, State};
use crate::{identity, ssh};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Msg {
    Hello { info: LocalInfo, pubkey: String },
    Confirm { ok: bool },
}

pub fn code_for(a_pub: &str, b_pub: &str) -> String {
    let (x, y) = if a_pub <= b_pub { (a_pub, b_pub) } else { (b_pub, a_pub) };
    let h = Sha256::digest(format!("twin-pair\n{}\n{}\n", x.trim(), y.trim()).as_bytes());
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) % 1_000_000;
    format!("{n:06}")
}

fn send(s: &mut TcpStream, m: &Msg) -> Result<()> {
    s.write_all(serde_json::to_string(m)?.as_bytes())?;
    s.write_all(b"\n")?;
    Ok(())
}
fn recv(r: &mut BufReader<TcpStream>) -> Result<Msg> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        bail!("peer closed connection");
    }
    Ok(serde_json::from_str(&line)?)
}

fn finish(their: &LocalInfo, their_key: &str, addr: &str) -> Result<PeerConfig> {
    identity::ensure()?;
    ssh::authorize_key(their_key)?;
    ssh::write_peer_host(addr, &their.user, ssh::identity_file_string().to_str().unwrap())?;
    let pc = PeerConfig {
        name: their.name.clone(),
        host: their.host.clone(),
        os: their.os.clone(),
        user: their.user.clone(),
        home: their.home.clone(),
        addr: addr.to_string(),
        hub: their.os == "linux",
        fp: identity::fingerprint(their_key),
    };
    let mut cfg = Config::load()?;
    cfg.peer = Some(pc.clone());
    cfg.save()?;
    Ok(pc)
}

/// Server side. `accept(code)` is asked once per incoming handshake; return true to confirm.
pub fn serve(port: u16, accept: Box<dyn Fn(&str) -> bool + Send + Sync>, emitter: &dyn Emitter) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).with_context(|| format!("bind port {port}"))?;
    let me = local_info();
    let id = identity::ensure()?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let addr = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
        let mut w = match stream.try_clone() {
            Ok(w) => w,
            Err(_) => continue,
        };
        let mut r = BufReader::new(stream);
        let result = (|| -> Result<()> {
            let Msg::Hello { info: their, pubkey: their_key } = recv(&mut r)? else { bail!("expected hello") };
            send(&mut w, &Msg::Hello { info: me.clone(), pubkey: id.public_key.clone() })?;
            let code = code_for(&id.public_key, &their_key);
            emitter.emit(Event::Code { code: code.clone() });
            let mine = accept(&code);
            send(&mut w, &Msg::Confirm { ok: mine })?;
            let Msg::Confirm { ok: theirs } = recv(&mut r)? else { bail!("expected confirm") };
            if !(mine && theirs) {
                bail!("pairing declined")
            }
            let pc = finish(&their, &their_key, &addr)?;
            emitter.emit(Event::Step { id: "pair".into(), state: State::Ok, msg: format!("paired with {}", pc.name) });
            Ok(())
        })();
        if let Err(e) = result {
            emitter.emit(Event::Step { id: "pair".into(), state: State::Fail, msg: e.to_string() });
        }
    }
    Ok(())
}

/// Client side.
pub fn connect(peer: &Found, confirm: Box<dyn Fn(&str) -> bool>, emitter: &dyn Emitter) -> Result<PeerConfig> {
    let me = local_info();
    let id = identity::ensure()?;
    let stream = TcpStream::connect((peer.addr.as_str(), peer.port))
        .with_context(|| format!("connect {}:{}", peer.addr, peer.port))?;
    let mut w = stream.try_clone()?;
    let mut r = BufReader::new(stream);
    send(&mut w, &Msg::Hello { info: me, pubkey: id.public_key.clone() })?;
    let Msg::Hello { info: their, pubkey: their_key } = recv(&mut r)? else { bail!("expected hello") };
    let code = code_for(&id.public_key, &their_key);
    emitter.emit(Event::Code { code: code.clone() });
    let mine = confirm(&code);
    send(&mut w, &Msg::Confirm { ok: mine })?;
    let Msg::Confirm { ok: theirs } = recv(&mut r)? else { bail!("expected confirm") };
    if !(mine && theirs) {
        bail!("pairing declined")
    }
    let pc = finish(&their, &their_key, &peer.addr)?;
    emitter.emit(Event::Step { id: "pair".into(), state: State::Ok, msg: format!("paired with {}", pc.name) });
    Ok(pc)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_is_six_digits_and_symmetric() {
        let a = "ssh-ed25519 AAAA a";
        let b = "ssh-ed25519 BBBB b";
        let c = code_for(a, b);
        assert_eq!(c.len(), 6);
        assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(c, code_for(b, a));
        assert_ne!(c, code_for(a, "ssh-ed25519 CCCC c"));
    }
}
