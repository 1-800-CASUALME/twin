use crate::app::{App, Step};
use crate::icons;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap};
use ratatui::Frame;
use twin_core::diagnose::human;
use twin_core::event::State;

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

fn state_span(s: &State, tick: usize) -> Span<'static> {
    match s {
        State::Pending => Span::styled(icons::dot(), Style::default().fg(MUTED)),
        State::Running => Span::styled(icons::spinner(tick), Style::default().fg(ACCENT)),
        State::Ok => Span::styled(icons::check(), Style::default().fg(Color::Green)),
        State::Warn => Span::styled(icons::warn(), Style::default().fg(Color::Yellow)),
        State::Fail => Span::styled(icons::cross(), Style::default().fg(Color::Red)),
        State::Skipped => Span::styled("-", Style::default().fg(MUTED)),
    }
}

fn card(title: &str, focused: bool, selected: bool) -> Block<'_> {
    let border = if focused { ACCENT } else if selected { Color::Green } else { MUTED };
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(border))
        .title(Span::styled(format!(" {title} "), Style::default().add_modifier(Modifier::BOLD)))
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect { x, y, width: w.min(area.width), height: h.min(area.height) }
}

fn big_button(label: &str, busy: bool, tick: usize) -> Line<'static> {
    let icon = if busy { icons::spinner(tick).to_string() } else { icons::arrow().to_string() };
    Line::from(vec![
        Span::styled(format!("  {icon} {label}  "), Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD)),
    ])
    .alignment(Alignment::Center)
}

pub fn draw(f: &mut Frame, app: &App) {
    let [side, main] = Layout::horizontal([Constraint::Length(22), Constraint::Min(30)]).areas(f.area());
    let [content, hints] = Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(main);
    sidebar(f, app, side);
    let block = Block::default().borders(Borders::LEFT).border_style(Style::default().fg(MUTED));
    let inner = block.inner(content);
    f.render_widget(block, content);
    let inner = Rect { x: inner.x + 2, y: inner.y + 1, width: inner.width.saturating_sub(4), height: inner.height.saturating_sub(2) };
    match app.step {
        Step::Welcome => welcome(f, inner),
        Step::Connect => connect(f, app, inner),
        Step::Diagnose => diagnose(f, app, inner),
        Step::Choose => choose(f, app, inner),
        Step::Sync => sync(f, app, inner),
        Step::Done => done(f, app, inner),
    }
    hint_bar(f, app, hints);
    if let Some(code) = &app.code {
        code_modal(f, code, app.tick, f.area());
    }
}

fn sidebar(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![
        Line::from(vec![Span::styled(format!(" {} ", icons::logo()), Style::default().fg(ACCENT)), Span::styled("Twin", Style::default().add_modifier(Modifier::BOLD))]),
        Line::from(""),
    ];
    for s in Step::ALL {
        let (glyph, style) = if s < app.step {
            (icons::check(), Style::default().fg(Color::Green))
        } else if s == app.step {
            (icons::arrow(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        } else {
            (icons::dot(), Style::default().fg(MUTED))
        };
        let title_style = if s == app.step { Style::default().add_modifier(Modifier::BOLD) } else if s < app.step { Style::default() } else { Style::default().fg(MUTED) };
        lines.push(Line::from(vec![Span::styled(format!("  {glyph} "), style), Span::styled(s.title(), title_style)]));
    }
    if let Some(p) = &app.paired {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(format!("  {} ", icons::connect()), Style::default().fg(MUTED)), Span::styled(p.clone(), Style::default().fg(MUTED))]));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn hint_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut h: Vec<(&str, &str)> = vec![];
    match app.step {
        Step::Welcome => h.push(("enter", "continue")),
        Step::Connect => {
            h.push(("enter", if app.paired.is_some() { "continue" } else if app.peers.is_empty() { "find" } else { "pair" }));
            h.push(("r", "find again"));
        }
        Step::Diagnose => {
            h.push(("enter", if app.diagnosed { "continue" } else { "check" }));
            h.push(("f", "fix"));
            h.push(("r", "check again"));
        }
        Step::Choose => {
            h.push(("space", "toggle"));
            h.push(("a", "all"));
            h.push(("enter", "expand"));
            h.push(("c", "continue"));
        }
        Step::Sync => h.push(("enter", if app.synced { "continue" } else { "sync" })),
        Step::Done => {
            h.push(("enter", "sync again"));
            h.push(("s", "schedule"));
        }
    }
    if app.step != Step::Welcome {
        h.push(("b", "back"));
    }
    h.push(("q", "quit"));
    let mut spans = vec![];
    for (k, v) in h {
        spans.push(Span::styled(format!(" {k} "), Style::default().fg(Color::Black).bg(MUTED)));
        spans.push(Span::styled(format!(" {v}  "), Style::default().fg(MUTED)));
    }
    if let Some(e) = &app.error {
        spans.push(Span::styled(format!(" {} {e}", icons::warn()), Style::default().fg(Color::Yellow)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn welcome(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(icons::logo(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled("Twin", Style::default().add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(Span::styled("Two machines. One workspace.", Style::default().fg(MUTED))).alignment(Alignment::Center),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("{} Connect", icons::connect()), Style::default().fg(ACCENT)),
            Span::raw("    "),
            Span::styled(format!("{} Diagnose", icons::diagnose()), Style::default().fg(ACCENT)),
            Span::raw("    "),
            Span::styled(format!("{} Sync", icons::sync()), Style::default().fg(ACCENT)),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(""),
        big_button("Continue", false, 0),
    ];
    f.render_widget(Paragraph::new(lines), centered(area, area.width.min(60), 14));
}

fn connect(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from("")];
    if let Some(p) = &app.paired {
        lines.push(Line::from(Span::styled(icons::check(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))).alignment(Alignment::Center));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!("Connected to {p}"), Style::default().add_modifier(Modifier::BOLD))).alignment(Alignment::Center));
        lines.push(Line::from(""));
        lines.push(big_button("Continue", false, 0));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("r  pair a different machine", Style::default().fg(MUTED))).alignment(Alignment::Center));
        f.render_widget(Paragraph::new(lines), centered(area, area.width.min(60), 10));
        return;
    }
    lines.push(Line::from(Span::styled(icons::wifi(), Style::default().fg(ACCENT))).alignment(Alignment::Center));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Find the other machine", Style::default().add_modifier(Modifier::BOLD))).alignment(Alignment::Center));
    lines.push(Line::from(Span::styled("Open Twin on both machines on the same network.", Style::default().fg(MUTED))).alignment(Alignment::Center));
    lines.push(Line::from(""));
    lines.push(big_button(if app.busy { "Looking…" } else { "Find" }, app.busy, app.tick));
    lines.push(Line::from(""));
    let head = Paragraph::new(lines);
    let [top, list] = Layout::vertical([Constraint::Length(9), Constraint::Min(3)]).areas(area);
    f.render_widget(head, top);
    let mut y = list.y;
    for (i, p) in app.peers.iter().enumerate() {
        if y + 3 > list.y + list.height {
            break;
        }
        let r = Rect { x: list.x + list.width.saturating_sub(50) / 2, y, width: 50.min(list.width), height: 3 };
        let b = card("", i == app.focus, false);
        let inner = b.inner(r);
        f.render_widget(b, r);
        let icon = if p.os == "linux" { icons::desktop() } else { icons::laptop() };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {icon}  "), Style::default().fg(ACCENT)),
                Span::styled(p.name.clone(), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("   {} · {}", p.os, p.addr), Style::default().fg(MUTED)),
            ])),
            inner,
        );
        y += 3;
    }
    if app.peers.is_empty() && !app.busy {
        f.render_widget(Paragraph::new(Span::styled("Nothing found yet.", Style::default().fg(MUTED))).alignment(Alignment::Center), list);
    }
}

fn code_modal(f: &mut Frame, code: &str, tick: usize, area: Rect) {
    let r = centered(area, 44, 9);
    f.render_widget(Clear, r);
    let b = Block::default().borders(Borders::ALL).border_type(BorderType::Double).border_style(Style::default().fg(ACCENT)).title(" Pairing ");
    let inner = b.inner(r);
    f.render_widget(b, r);
    let spaced: String = code.chars().map(|c| format!("{c} ")).collect();
    let lines = vec![
        Line::from(Span::styled(format!("{} Same code on both screens?", icons::lock()), Style::default().add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(spaced.trim_end().to_string(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::styled(" y ", Style::default().fg(Color::Black).bg(Color::Green)),
            Span::raw(" confirm     "),
            Span::styled(" n ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::raw(" cancel "),
            Span::styled(icons::spinner(tick), Style::default().fg(MUTED)),
        ])
        .alignment(Alignment::Center),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn grid(area: Rect, cols: u16, card_h: u16) -> Vec<Rect> {
    let cols = cols.max(1);
    let w = area.width / cols;
    let rows = area.height / card_h;
    let mut v = vec![];
    for r in 0..rows {
        for c in 0..cols {
            v.push(Rect { x: area.x + c * w, y: area.y + r * card_h, width: w.saturating_sub(1), height: card_h });
        }
    }
    v
}

fn diagnose(f: &mut Frame, app: &App, area: Rect) {
    let [top, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).areas(area);
    let title = Line::from(vec![
        Span::styled("Both machines", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled(
            if app.busy { format!("{} checking…", icons::spinner(app.tick)) } else if app.diagnosed { "done".into() } else { "press enter to check".into() },
            Style::default().fg(MUTED),
        ),
    ]);
    f.render_widget(Paragraph::new(title), top);
    let checks = app.sorted_checks();
    let cols = (body.width / 26).clamp(1, 4);
    let cells = grid(body, cols, 5);
    for (i, c) in checks.iter().enumerate() {
        let Some(r) = cells.get(i) else { break };
        let b = card(&c.name, i == app.focus, false);
        let inner = b.inner(*r);
        f.render_widget(b, *r);
        let side = match c.side.as_str() {
            "local" => "this machine",
            "peer" => "other machine",
            _ => "pair",
        };
        let mut l2 = vec![Span::styled(c.msg.clone(), Style::default().fg(MUTED))];
        if c.fixable && c.side != "peer" {
            l2.push(Span::styled("  f fix", Style::default().fg(ACCENT)));
        }
        let p = Paragraph::new(vec![
            Line::from(vec![Span::styled(format!("{} ", icons::for_id(&c.id)), Style::default().fg(ACCENT)), state_span(&c.state, app.tick), Span::raw(" "), Span::styled(side, Style::default().fg(MUTED))]),
            Line::from(l2),
        ])
        .wrap(Wrap { trim: true });
        f.render_widget(p, inner);
    }
}

fn choose(f: &mut Frame, app: &App, area: Rect) {
    let [top, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
    let title = Line::from(vec![
        Span::styled("What to sync", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled(format!("{} select all", icons::checkbox(app.all_selected())), Style::default().fg(if app.all_selected() { Color::Green } else { MUTED })),
        Span::raw("   "),
        Span::styled(if app.busy { format!("{} measuring…", icons::spinner(app.tick)) } else { String::new() }, Style::default().fg(MUTED)),
    ]);
    f.render_widget(Paragraph::new(title), top);
    let mut y = body.y;
    for (i, it) in app.items.iter().enumerate() {
        let sel = app.item_selected(it);
        let expanded = app.expanded.contains(&it.id);
        let h = if expanded { (it.members.len() as u16 + 3).min(body.y + body.height - y) } else { 3 };
        if y + 3 > body.y + body.height {
            break;
        }
        let r = Rect { x: body.x, y, width: body.width, height: h };
        let b = card("", i == app.focus, sel);
        let inner = b.inner(r);
        f.render_widget(b, r);
        let mut lines = vec![Line::from(vec![
            Span::styled(format!(" {} ", icons::checkbox(sel)), Style::default().fg(if sel { Color::Green } else { MUTED })),
            Span::styled(format!("{} ", icons::for_id(&it.id)), Style::default().fg(ACCENT)),
            Span::styled(it.name.clone(), Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("   {} · {}   {}", it.count, human(it.bytes), it.location), Style::default().fg(MUTED)),
            Span::styled(if expanded { "   enter hide" } else { "   enter show" }, Style::default().fg(MUTED)),
        ])];
        if expanded {
            for m in &it.members {
                let on = app.selected.contains(&App::key(&it.id, &m.id));
                let where_ = format!("{}{}", if m.local { icons::laptop() } else { " " }, if m.peer { icons::desktop() } else { " " });
                let mut spans = vec![
                    Span::styled(format!("     {} ", icons::checkbox(on)), Style::default().fg(if on { Color::Green } else { MUTED })),
                    Span::raw(m.name.clone()),
                    Span::styled(format!("  {}  ", m.detail), Style::default().fg(MUTED)),
                    Span::styled(where_, Style::default().fg(ACCENT)),
                    Span::styled(format!("  {}", human(m.bytes)), Style::default().fg(MUTED)),
                ];
                if it.id == "git" && app.autocommit.contains(&m.id) {
                    spans.push(Span::styled("  auto-commit", Style::default().fg(Color::Yellow)));
                }
                lines.push(Line::from(spans));
            }
        }
        f.render_widget(Paragraph::new(lines), inner);
        y += h;
    }
    let _ = y;
}

fn sync(f: &mut Frame, app: &App, area: Rect) {
    let [top, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).areas(area);
    let mut head = vec![Line::from(Span::styled("Sync", Style::default().add_modifier(Modifier::BOLD)))];
    if app.step_states.is_empty() && !app.busy {
        head.push(big_button("Sync", false, 0));
    } else if app.busy {
        head.push(Line::from(Span::styled(format!("{} syncing…", icons::spinner(app.tick)), Style::default().fg(ACCENT))));
    } else {
        head.push(big_button("Continue", false, 0));
    }
    f.render_widget(Paragraph::new(head), top);
    let mut y = body.y;
    for id in app.selected_items() {
        let subs: Vec<(&String, &(State, String))> = app.step_states.iter().filter(|(k, _)| k.starts_with(&format!("{id}:"))).collect();
        let h = (3 + subs.len() as u16 + 1).min(body.y + body.height - y);
        if y + 3 > body.y + body.height {
            break;
        }
        let r = Rect { x: body.x, y, width: body.width, height: h };
        let b = card("", false, false);
        let inner = b.inner(r);
        f.render_widget(b, r);
        let st = app.step_states.get(&id);
        let mut lines = vec![Line::from(vec![
            Span::styled(format!(" {} ", icons::for_id(&id)), Style::default().fg(ACCENT)),
            st.map(|s| state_span(&s.0, app.tick)).unwrap_or(state_span(&State::Pending, 0)),
            Span::raw(" "),
            Span::styled(app.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or(id.clone()), Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("   {}", st.map(|s| s.1.clone()).unwrap_or("waiting…".into())), Style::default().fg(MUTED)),
        ])];
        for (k, v) in &subs {
            lines.push(Line::from(vec![Span::raw("     "), state_span(&v.0, app.tick), Span::raw(format!(" {}  ", &k[id.len() + 1..])), Span::styled(v.1.clone(), Style::default().fg(MUTED))]));
        }
        f.render_widget(Paragraph::new(lines), inner);
        if let (Some((done, total)), Some((State::Running, _))) = (app.progress.get(&id), st) {
            if *total > 0 && inner.height >= 2 {
                let g = Gauge::default().gauge_style(Style::default().fg(ACCENT).bg(Color::Black)).ratio(*done as f64 / *total as f64).label(format!("{done}/{total}"));
                f.render_widget(g, Rect { x: inner.x + 1, y: inner.y + inner.height - 1, width: inner.width.saturating_sub(2), height: 1 });
            }
        }
        y += h;
    }
    if !app.conflicts.is_empty() && y + 2 < body.y + body.height {
        let mut lines = vec![Line::from(Span::styled(format!("{} {} conflicts kept as copies", icons::warn(), app.conflicts.len()), Style::default().fg(Color::Yellow)))];
        for c in app.conflicts.iter().take((body.y + body.height - y) as usize - 1) {
            lines.push(Line::from(Span::styled(format!("   {}", c.path), Style::default().fg(MUTED))));
        }
        f.render_widget(Paragraph::new(lines), Rect { x: body.x, y, width: body.width, height: body.y + body.height - y });
    }
}

fn done(f: &mut Frame, app: &App, area: Rect) {
    let (glyph, color, title) = if app.sync_ok { (icons::check(), Color::Green, "In sync") } else { (icons::warn(), Color::Yellow, "Finished with issues") };
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(glyph, Style::default().fg(color).add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        Line::from(""),
    ];
    for id in app.selected_items() {
        if let Some((_, msg)) = app.step_states.get(&id) {
            lines.push(Line::from(Span::styled(format!("{}: {}", id, msg), Style::default().fg(MUTED))).alignment(Alignment::Center));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!("{} ", icons::checkbox(app.schedule)), Style::default().fg(if app.schedule { Color::Green } else { MUTED })),
        Span::raw("Keep in sync every 15 minutes"),
        Span::styled("   s toggle", Style::default().fg(MUTED)),
    ]).alignment(Alignment::Center));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Attach to the other machine with:  twin attach", Style::default().fg(MUTED))).alignment(Alignment::Center));
    lines.push(Line::from(""));
    lines.push(big_button("Sync again", false, 0));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), centered(area, area.width.min(70), 16));
}
