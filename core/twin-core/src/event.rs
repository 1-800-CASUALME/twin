use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Pending,
    Running,
    Ok,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ev", rename_all = "lowercase")]
pub enum Event {
    Step { id: String, state: State, msg: String },
    Progress { id: String, done: u64, total: u64, bytes: u64 },
    Conflict { id: String, path: String, kept: String },
    Peer { name: String, host: String, os: String, addr: String, port: u16, instance: String },
    Code { code: String },
    Item(crate::inventory::Item),
    Check(crate::diagnose::CheckResult),
    Done { ok: bool },
    Error { msg: String },
}

pub trait Emitter: Send + Sync {
    fn emit(&self, ev: Event);
}

pub struct StdoutEmitter;
impl Emitter for StdoutEmitter {
    fn emit(&self, ev: Event) {
        println!("{}", serde_json::to_string(&ev).expect("event serializes"));
    }
}

pub struct ChannelEmitter(pub Sender<Event>);
impl Emitter for ChannelEmitter {
    fn emit(&self, ev: Event) {
        let _ = self.0.send(ev);
    }
}

pub struct NullEmitter;
impl Emitter for NullEmitter {
    fn emit(&self, _: Event) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn step_serializes_with_ev_tag() {
        let ev = Event::Step { id: "claude".into(), state: State::Running, msg: "comparing".into() };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"ev":"step","id":"claude","state":"running","msg":"comparing"}"#);
    }
    #[test]
    fn channel_emitter_delivers() {
        let (tx, rx) = std::sync::mpsc::channel();
        ChannelEmitter(tx).emit(Event::Done { ok: true });
        assert_eq!(rx.recv().unwrap(), Event::Done { ok: true });
    }
}
