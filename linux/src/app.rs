use std::collections::{BTreeMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use twin_core::config::Config;
use twin_core::diagnose::CheckResult;
use twin_core::discover::Found;
use twin_core::event::{Emitter, Event, State};
use twin_core::inventory::Item;
use twin_core::ssh::Peer;
use twin_core::{diagnose, discover, engines, inventory, lock::RunLock, log::RunLog, pair};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome,
    Connect,
    Diagnose,
    Choose,
    Sync,
    Done,
}
impl Step {
    pub const ALL: [Step; 6] = [Step::Welcome, Step::Connect, Step::Diagnose, Step::Choose, Step::Sync, Step::Done];
    pub fn title(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome",
            Step::Connect => "Connect",
            Step::Diagnose => "Diagnose",
            Step::Choose => "Choose",
            Step::Sync => "Sync",
            Step::Done => "Done",
        }
    }
    fn idx(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap()
    }
    pub fn next(self) -> Option<Step> {
        Step::ALL.get(self.idx() + 1).copied()
    }
    pub fn prev(self) -> Option<Step> {
        self.idx().checked_sub(1).map(|i| Step::ALL[i])
    }
}

pub enum Msg {
    Ev(Event),
    AskCode(String, Sender<bool>),
    Finished(&'static str),
}

struct MsgEmitter(Sender<Msg>);
impl Emitter for MsgEmitter {
    fn emit(&self, ev: Event) {
        let _ = self.0.send(Msg::Ev(ev));
    }
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub path: String,
    pub kept: String,
}

pub struct App {
    pub step: Step,
    pub busy: bool,
    pub error: Option<String>,
    pub tick: usize,
    pub focus: usize,
    pub expanded: HashSet<String>,
    // connect
    pub peers: Vec<Found>,
    pub code: Option<String>,
    pub code_reply: Option<Sender<bool>>,
    pub pair_reply: Option<Sender<bool>>,
    pub paired: Option<String>,
    // diagnose
    pub checks: BTreeMap<String, CheckResult>,
    pub diagnosed: bool,
    // choose
    pub items: Vec<Item>,
    pub selected: HashSet<String>, // "item:member"
    pub autocommit: HashSet<String>,
    // sync
    pub step_states: BTreeMap<String, (State, String)>,
    pub progress: BTreeMap<String, (u64, u64)>,
    pub conflicts: Vec<Conflict>,
    pub synced: bool,
    pub sync_ok: bool,
    pub schedule: bool,
    pub quit: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    pub fn new() -> App {
        let (tx, rx) = channel();
        let mut app = App {
            step: Step::Welcome,
            busy: false,
            error: None,
            tick: 0,
            focus: 0,
            expanded: HashSet::new(),
            peers: vec![],
            code: None,
            code_reply: None,
            pair_reply: None,
            paired: None,
            checks: BTreeMap::new(),
            diagnosed: false,
            items: vec![],
            selected: HashSet::new(),
            autocommit: HashSet::new(),
            step_states: BTreeMap::new(),
            progress: BTreeMap::new(),
            conflicts: vec![],
            synced: false,
            sync_ok: false,
            schedule: twin_core::schedule::is_on(),
            quit: false,
            tx,
            rx,
        };
        app.load_status();
        app.start_daemon();
        app
    }

    fn load_status(&mut self) {
        if let Ok(cfg) = Config::load() {
            if let Some(p) = cfg.peer {
                self.paired = Some(p.name);
            }
        }
    }

    fn spawn(&self, label: &'static str, f: impl FnOnce(&dyn Emitter) + Send + 'static) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let em = MsgEmitter(tx.clone());
            f(&em);
            let _ = tx.send(Msg::Finished(label));
        });
    }

    /// Advertise on the LAN and accept pairing requests for the app's lifetime.
    fn start_daemon(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            let info = discover::local_info();
            let _adv = match discover::advertise(7423, &info) {
                Ok(a) => Some(a),
                Err(e) => {
                    let _ = tx.send(Msg::Ev(Event::Error { msg: format!("mDNS advertise failed: {e}") }));
                    None
                }
            };
            let ask_tx = tx.clone();
            let accept = Box::new(move |code: &str| -> bool {
                let (rtx, rrx) = channel();
                let _ = ask_tx.send(Msg::AskCode(code.to_string(), rtx));
                rrx.recv_timeout(Duration::from_secs(120)).unwrap_or(false)
            });
            let em = MsgEmitter(tx.clone());
            if let Err(e) = pair::serve(7423, accept, &em) {
                let _ = tx.send(Msg::Ev(Event::Error { msg: format!("pairing daemon: {e}") }));
            }
        });
    }

    // ----- actions -----

    pub fn discover(&mut self) {
        self.busy = true;
        self.peers.clear();
        self.error = None;
        self.spawn("discover", |em| {
            let _ = discover::browse(Duration::from_secs(4), em);
        });
    }

    pub fn pair(&mut self, i: usize) {
        let Some(peer) = self.peers.get(i).cloned() else { return };
        self.busy = true;
        self.error = None;
        let (rtx, rrx) = channel::<bool>();
        self.pair_reply = Some(rtx);
        let tx = self.tx.clone();
        self.spawn("pair", move |em| {
            let confirm = Box::new(move |code: &str| -> bool {
                let _ = tx.send(Msg::Ev(Event::Code { code: code.to_string() }));
                rrx.recv_timeout(Duration::from_secs(120)).unwrap_or(false)
            });
            if let Err(e) = pair::connect(&peer, confirm, em) {
                em.emit(Event::Error { msg: e.to_string() });
            }
        });
    }

    pub fn answer(&mut self, yes: bool) {
        if let Some(r) = self.code_reply.take() {
            let _ = r.send(yes);
        }
        if let Some(r) = self.pair_reply.take() {
            let _ = r.send(yes);
        }
        self.code = None;
    }

    pub fn diagnose(&mut self) {
        self.busy = true;
        self.checks.clear();
        self.error = None;
        self.spawn("diagnose", |em| {
            if let Ok(l) = RunLog::start() {
                twin_core::cmd::set_log(l);
            }
            match Config::load() {
                Ok(cfg) => {
                    let _ = diagnose::run_all(&cfg, em);
                }
                Err(e) => em.emit(Event::Error { msg: e.to_string() }),
            }
        });
    }

    pub fn fix_focused(&mut self) {
        let checks: Vec<CheckResult> = self.sorted_checks();
        let Some(c) = checks.get(self.focus).cloned() else { return };
        if !c.fixable || c.side == "peer" {
            return;
        }
        if c.id == "ssh" {
            self.spawn("fix", |em| {
                let o = twin_core::cmd::run("sudo", &["-n", "systemctl", "enable", "--now", "sshd"], None);
                let ok = o.map(|o| o.status == 0).unwrap_or(false);
                em.emit(Event::Step {
                    id: "ssh".into(),
                    state: if ok { State::Ok } else { State::Fail },
                    msg: if ok { "sshd enabled".into() } else { "run: sudo systemctl enable --now sshd".into() },
                });
            });
            return;
        }
        self.busy = true;
        let id = c.id.clone();
        self.spawn("fix", move |em| {
            let _ = diagnose::fix(&id, em);
        });
    }

    pub fn sorted_checks(&self) -> Vec<CheckResult> {
        let order = |s: &str| match s {
            "local" => 0,
            "pair" => 1,
            _ => 2,
        };
        let mut v: Vec<CheckResult> = self.checks.values().cloned().collect();
        v.sort_by_key(|c| (order(&c.side), c.id.clone()));
        v
    }

    pub fn inventory(&mut self) {
        self.busy = true;
        self.items.clear();
        self.error = None;
        self.spawn("inventory", |em| match Config::load() {
            Ok(cfg) => {
                let _ = inventory::merged(&cfg, em);
            }
            Err(e) => em.emit(Event::Error { msg: e.to_string() }),
        });
    }

    pub fn key(item: &str, member: &str) -> String {
        format!("{item}:{member}")
    }
    pub fn item_selected(&self, it: &Item) -> bool {
        it.members.iter().any(|m| self.selected.contains(&Self::key(&it.id, &m.id)))
    }
    pub fn all_selected(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|i| i.members.iter().all(|m| self.selected.contains(&Self::key(&i.id, &m.id))))
    }
    pub fn select_all(&mut self, on: bool) {
        self.selected.clear();
        if on {
            for i in &self.items {
                for m in &i.members {
                    self.selected.insert(Self::key(&i.id, &m.id));
                }
            }
        }
    }
    pub fn toggle_item(&mut self, idx: usize) {
        let Some(it) = self.items.get(idx).cloned() else { return };
        let on = !self.item_selected(&it);
        for m in &it.members {
            let k = Self::key(&it.id, &m.id);
            if on {
                self.selected.insert(k);
            } else {
                self.selected.remove(&k);
            }
        }
    }
    pub fn toggle_member(&mut self, item: &str, member: &str) {
        let k = Self::key(item, member);
        if !self.selected.remove(&k) {
            self.selected.insert(k);
        }
    }
    pub fn selected_items(&self) -> Vec<String> {
        self.items.iter().filter(|i| self.item_selected(i)).map(|i| i.id.clone()).collect()
    }

    pub fn sync(&mut self) {
        self.busy = true;
        self.error = None;
        self.step_states.clear();
        self.progress.clear();
        self.conflicts.clear();
        self.synced = false;
        let ids = self.selected_items();
        let mut members: Vec<String> = vec![];
        for i in &self.items {
            let all = i.members.iter().all(|m| self.selected.contains(&Self::key(&i.id, &m.id)));
            if !all {
                for m in &i.members {
                    let k = Self::key(&i.id, &m.id);
                    if self.selected.contains(&k) {
                        members.push(k);
                    }
                }
            }
        }
        let autocommit: Vec<String> = self.autocommit.iter().cloned().collect();
        self.spawn("sync", move |em| {
            let run = || -> anyhow::Result<bool> {
                let _lock = RunLock::acquire()?;
                twin_core::cmd::set_log(RunLog::start()?);
                let mut cfg = Config::load()?;
                cfg.selection = ids.clone();
                cfg.members = members.clone();
                cfg.git_autocommit = autocommit.clone();
                cfg.save()?;
                let peer = Peer::new(&cfg)?;
                if !peer.reachable() {
                    anyhow::bail!("peer {} unreachable", peer.name);
                }
                let mut ok = true;
                for id in &ids {
                    let Some(e) = engines::by_id(id) else { continue };
                    let mine: Vec<String> = members.iter().filter_map(|m| m.strip_prefix(&format!("{id}:")).map(String::from)).collect();
                    if let Err(err) = e.sync(&cfg, &peer, &mine, em) {
                        ok = false;
                        em.emit(Event::Step { id: id.clone(), state: State::Fail, msg: format!("{err:#}") });
                    }
                }
                Ok(ok)
            };
            match run() {
                Ok(ok) => em.emit(Event::Done { ok }),
                Err(e) => {
                    em.emit(Event::Error { msg: format!("{e:#}") });
                    em.emit(Event::Done { ok: false });
                }
            }
        });
    }

    pub fn toggle_schedule(&mut self) {
        let r = if self.schedule { twin_core::schedule::off() } else { twin_core::schedule::on() };
        match r {
            Ok(_) => self.schedule = !self.schedule,
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn can_continue(&self) -> bool {
        match self.step {
            Step::Welcome => true,
            Step::Connect => self.paired.is_some(),
            Step::Diagnose => self.diagnosed && !self.checks.values().any(|c| c.state == State::Fail),
            Step::Choose => !self.selected.is_empty(),
            Step::Sync => self.synced,
            Step::Done => false,
        }
    }

    pub fn next(&mut self) {
        if !self.can_continue() || self.busy {
            return;
        }
        if let Some(n) = self.step.next() {
            self.step = n;
            self.focus = 0;
            match n {
                Step::Diagnose if !self.diagnosed => self.diagnose(),
                Step::Choose if self.items.is_empty() => self.inventory(),
                _ => {}
            }
        }
    }
    pub fn back(&mut self) {
        if self.busy {
            return;
        }
        if let Some(p) = self.step.prev() {
            self.step = p;
            self.focus = 0;
        }
    }
    pub fn restart(&mut self) {
        self.synced = false;
        self.sync_ok = false;
        self.step_states.clear();
        self.progress.clear();
        self.conflicts.clear();
        self.step = Step::Choose;
        self.focus = 0;
    }

    /// Primary action for Enter on the current step.
    pub fn primary(&mut self) {
        match self.step {
            Step::Welcome => self.next(),
            Step::Connect => {
                if self.paired.is_some() && !self.busy {
                    self.next();
                } else if !self.peers.is_empty() && self.focus < self.peers.len() {
                    self.pair(self.focus);
                } else if !self.busy {
                    self.discover();
                }
            }
            Step::Diagnose => {
                if self.diagnosed {
                    self.next()
                } else if !self.busy {
                    self.diagnose()
                }
            }
            Step::Choose => {
                if let Some(it) = self.items.get(self.focus) {
                    let id = it.id.clone();
                    if !self.expanded.remove(&id) {
                        self.expanded.insert(id);
                    }
                }
            }
            Step::Sync => {
                if self.synced {
                    self.next()
                } else if !self.busy {
                    self.sync()
                }
            }
            Step::Done => self.restart(),
        }
    }

    pub fn focus_len(&self) -> usize {
        match self.step {
            Step::Connect => self.peers.len(),
            Step::Diagnose => self.checks.len(),
            Step::Choose => self.items.len(),
            _ => 0,
        }
    }

    /// Drain background messages. Call every tick.
    pub fn pump(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::AskCode(code, reply) => {
                    self.code = Some(code);
                    self.code_reply = Some(reply);
                }
                Msg::Finished(label) => {
                    self.busy = false;
                    match label {
                        "diagnose" => self.diagnosed = true,
                        "fix" => self.diagnose(),
                        "inventory" => {
                            if self.selected.is_empty() {
                                self.select_all(true)
                            }
                        }
                        "sync" => self.synced = true,
                        _ => {}
                    }
                }
                Msg::Ev(ev) => match ev {
                    Event::Peer { name, host, os, addr, port, instance } => {
                        if !self.peers.iter().any(|p| p.instance == instance) {
                            self.peers.push(Found { name, host, os, user: String::new(), home: String::new(), addr, port, instance });
                        }
                    }
                    Event::Code { code } => self.code = Some(code),
                    Event::Step { id, state, msg } => {
                        if id == "pair" {
                            self.code = None;
                            if state == State::Ok {
                                self.paired = Some(msg.trim_start_matches("paired with ").to_string());
                                self.load_status();
                            } else {
                                self.error = Some(msg);
                            }
                        } else if id == "ssh" && self.step == Step::Diagnose {
                            if let Some(c) = self.checks.get_mut("local:ssh") {
                                c.state = state;
                                c.msg = msg;
                            }
                        } else {
                            self.step_states.insert(id, (state, msg));
                        }
                    }
                    Event::Check(c) => {
                        self.checks.insert(format!("{}:{}", c.side, c.id), c);
                    }
                    Event::Item(i) => self.items.push(i),
                    Event::Progress { id, done, total, .. } => {
                        self.progress.insert(id, (done, total));
                    }
                    Event::Conflict { path, kept, .. } => self.conflicts.push(Conflict { path, kept }),
                    Event::Done { ok } => {
                        if self.step == Step::Sync {
                            self.sync_ok = ok
                        }
                    }
                    Event::Error { msg } => self.error = Some(msg),
                },
            }
        }
    }
}
