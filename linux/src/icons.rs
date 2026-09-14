//! Nerd Font glyphs (Omarchy ships CaskaydiaMono Nerd Font) with an ASCII fallback.

use std::sync::atomic::{AtomicBool, Ordering};

static ASCII: AtomicBool = AtomicBool::new(false);
pub fn set_ascii(v: bool) {
    ASCII.store(v, Ordering::Relaxed);
}
fn pick(nerd: &'static str, ascii: &'static str) -> &'static str {
    if ASCII.load(Ordering::Relaxed) { ascii } else { nerd }
}

pub fn logo() -> &'static str { pick("󰓦", "<>") }
pub fn welcome() -> &'static str { pick("󰋜", "*") }
pub fn connect() -> &'static str { pick("󰌷", "@") }
pub fn diagnose() -> &'static str { pick("󰓙", "?") }
pub fn choose() -> &'static str { pick("󰄵", "#") }
pub fn sync() -> &'static str { pick("󰓦", "~") }
pub fn done() -> &'static str { pick("󰄬", "=") }
pub fn check() -> &'static str { pick("󰄬", "+") }
pub fn cross() -> &'static str { pick("󰅖", "x") }
pub fn warn() -> &'static str { pick("󰀦", "!") }
pub fn dot() -> &'static str { pick("󰝦", "o") }
pub fn arrow() -> &'static str { pick("󰅂", ">") }
pub fn laptop() -> &'static str { pick("󰌢", "L") }
pub fn desktop() -> &'static str { pick("󰍹", "D") }
pub fn wifi() -> &'static str { pick("󰖩", "(( ))") }
pub fn lock() -> &'static str { pick("󰌾", "#") }
pub fn checkbox(on: bool) -> &'static str {
    if on { pick("󰄲", "[x]") } else { pick("󰄱", "[ ]") }
}

pub fn spinner(tick: usize) -> &'static str {
    const N: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const A: [&str; 4] = ["|", "/", "-", "\\"];
    if ASCII.load(Ordering::Relaxed) { A[tick % 4] } else { N[tick % 10] }
}

/// Icon for a syncable item or diagnose check, by its core id.
pub fn for_id(id: &str) -> &'static str {
    match id {
        "claude" => pick("󱚝", "C"),
        "git" => pick("󰊢", "G"),
        "dotfiles" => pick("󰈙", "."),
        "history" => pick("󰋚", "H"),
        "terminal" | "tmux" => pick("󰆍", "T"),
        "folders" => pick("󰉋", "F"),
        "ssh" => pick("󰣀", "S"),
        "rsync" => pick("󰓦", "R"),
        "et" => pick("󱐋", "E"),
        "atuin" => pick("󰋚", "A"),
        "chezmoi" => pick("󰈙", "Z"),
        "disk" => pick("󰋊", "D"),
        "conflicts" => pick("󰀦", "!"),
        "twin" => pick("󰓦", "~"),
        _ => pick("󰘥", "?"),
    }
}
