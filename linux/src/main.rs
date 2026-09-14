mod app;
mod icons;
mod ui;

use anyhow::Result;
use app::{App, Step};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use std::time::Duration;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--ascii") {
        icons::set_ascii(true);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("twin-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let mut app = App::new();
    let res = run(&mut terminal, &mut app);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        app.pump();
        terminal.draw(|f| ui::draw(f, app))?;
        if app.quit {
            return Ok(());
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        match event::read()? {
            CEvent::Key(k) if k.kind == KeyEventKind::Press => {
                if app.code.is_some() {
                    match k.code {
                        KeyCode::Char('y') | KeyCode::Enter => app.answer(true),
                        KeyCode::Char('n') | KeyCode::Esc => app.answer(false),
                        _ => {}
                    }
                    continue;
                }
                match (k.code, k.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
                    (KeyCode::Esc, _) => app.error = None,
                    (KeyCode::Enter, _) => app.primary(),
                    (KeyCode::Char('b'), _) | (KeyCode::Left, _) => app.back(),
                    (KeyCode::Char('c'), _) | (KeyCode::Right, _) => app.next(),
                    (KeyCode::Char('r'), _) => match app.step {
                        Step::Connect => {
                            app.paired = None;
                            app.discover();
                        }
                        Step::Diagnose => app.diagnose(),
                        Step::Choose => app.inventory(),
                        _ => {}
                    },
                    (KeyCode::Char('f'), _) if app.step == Step::Diagnose => app.fix_focused(),
                    (KeyCode::Char('a'), _) if app.step == Step::Choose => {
                        let on = !app.all_selected();
                        app.select_all(on);
                    }
                    (KeyCode::Char(' '), _) if app.step == Step::Choose => app.toggle_item(app.focus),
                    (KeyCode::Char('m'), _) if app.step == Step::Choose => {
                        if let Some(it) = app.items.get(app.focus).cloned() {
                            if it.id == "git" {
                                for m in &it.members {
                                    if !app.autocommit.remove(&m.id) {
                                        app.autocommit.insert(m.id.clone());
                                    }
                                }
                            }
                        }
                    }
                    (KeyCode::Down, _) | (KeyCode::Char('j'), _) | (KeyCode::Tab, _) => {
                        let n = app.focus_len();
                        if n > 0 {
                            app.focus = (app.focus + 1) % n;
                        }
                    }
                    (KeyCode::Up, _) | (KeyCode::Char('k'), _) | (KeyCode::BackTab, _) => {
                        let n = app.focus_len();
                        if n > 0 {
                            app.focus = (app.focus + n - 1) % n;
                        }
                    }
                    _ => {}
                }
            }
            CEvent::Mouse(m) => {
                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    // Left column: click a completed step to jump back to it.
                    if m.column < 22 {
                        let idx = (m.row as usize).saturating_sub(2);
                        if let Some(s) = Step::ALL.get(idx) {
                            if *s <= app.step && !app.busy {
                                app.step = *s;
                                app.focus = 0;
                            }
                        }
                    } else if app.step == Step::Choose {
                        // Cards are stacked; each collapsed card is 3 rows starting at row 3.
                        let mut y = 3u16;
                        for (i, it) in app.items.iter().enumerate() {
                            let h = if app.expanded.contains(&it.id) { it.members.len() as u16 + 3 } else { 3 };
                            if m.row >= y && m.row < y + h {
                                app.focus = i;
                                if m.row == y + 1 {
                                    app.toggle_item(i);
                                } else if m.row > y + 1 {
                                    let mi = (m.row - y - 2) as usize;
                                    if let Some(mem) = it.members.get(mi) {
                                        let (a, b) = (it.id.clone(), mem.id.clone());
                                        app.toggle_member(&a, &b);
                                    }
                                }
                                break;
                            }
                            y += h;
                        }
                    } else if app.step == Step::Connect && !app.peers.is_empty() {
                        let idx = ((m.row as usize).saturating_sub(10)) / 3;
                        if idx < app.peers.len() {
                            app.pair(idx);
                        }
                    } else {
                        app.primary();
                    }
                }
            }
            _ => {}
        }
    }
}
