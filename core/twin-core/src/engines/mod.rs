use crate::config::Config;
use crate::event::Emitter;
use crate::ssh::Peer;
use anyhow::Result;

pub mod claude;
pub mod git;

pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    /// `members` is the list of member ids selected on the Choose screen; empty means all.
    fn sync(&self, cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()>;
}

pub fn all() -> Vec<Box<dyn Engine>> {
    vec![Box::new(claude::ClaudeEngine), Box::new(git::GitEngine)]
}
pub fn by_id(id: &str) -> Option<Box<dyn Engine>> {
    all().into_iter().find(|e| e.id() == id)
}
