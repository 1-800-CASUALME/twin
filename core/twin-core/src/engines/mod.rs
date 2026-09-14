use crate::config::Config;
use crate::event::Emitter;
use crate::ssh::Peer;
use anyhow::Result;

pub mod claude;
pub mod dotfiles;
pub mod files;
pub mod folders;
pub mod git;
pub mod history;
pub mod terminal;

pub trait Engine: Send + Sync {
    fn id(&self) -> &'static str;
    /// `members` is the list of member ids selected on the Choose screen; empty means all.
    fn sync(&self, cfg: &Config, peer: &Peer, members: &[String], emitter: &dyn Emitter) -> Result<()>;
}

pub fn all() -> Vec<Box<dyn Engine>> {
    vec![
        Box::new(claude::ClaudeEngine),
        Box::new(git::GitEngine),
        Box::new(dotfiles::DotfilesEngine),
        Box::new(history::HistoryEngine),
        Box::new(terminal::TerminalEngine),
        Box::new(folders::FoldersEngine),
    ]
}
pub fn by_id(id: &str) -> Option<Box<dyn Engine>> {
    all().into_iter().find(|e| e.id() == id)
}
