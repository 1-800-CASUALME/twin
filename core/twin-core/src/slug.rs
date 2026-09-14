/// Claude Code derives the project folder name from the absolute cwd by
/// replacing every character that is not [A-Za-z0-9] with '-'.
pub fn from_path(p: &str) -> String {
    p.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

pub fn home_slug(home: &str) -> String {
    from_path(home.trim_end_matches('/'))
}

/// Rewrite the home-prefix slug. "-Users-asim-wagt" with homes
/// "/Users/asim" -> "/home/asim" becomes "-home-asim-wagt".
pub fn translate(slug: &str, from_home: &str, to_home: &str) -> Option<String> {
    let from = home_slug(from_home);
    let to = home_slug(to_home);
    if slug == from {
        return Some(to);
    }
    let rest = slug.strip_prefix(&from)?;
    if !rest.starts_with('-') {
        return None;
    }
    Some(format!("{to}{rest}"))
}

/// Readable location for UI: "~/Desktop-wagt-ai" (lossy; '-' may have been '.', ' ', '/').
pub fn display_path(slug: &str, home: &str) -> String {
    let hs = home_slug(home);
    match slug.strip_prefix(&hs) {
        Some("") => "~".to_string(),
        Some(rest) => format!("~{}", rest.replacen('-', "/", 1)),
        None => slug.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn from_path_matches_claude() {
        assert_eq!(from_path("/Users/asim/Desktop/wagt.ai"), "-Users-asim-Desktop-wagt-ai");
        assert_eq!(from_path("/Users/asim/Desktop/A07 IDE"), "-Users-asim-Desktop-A07-IDE");
        assert_eq!(from_path("/home/asim"), "-home-asim");
    }
    #[test]
    fn translate_swaps_home_prefix_only() {
        assert_eq!(translate("-Users-asim-wagt", "/Users/asim", "/home/asim").unwrap(), "-home-asim-wagt");
        assert_eq!(translate("-Users-asim", "/Users/asim", "/home/asim").unwrap(), "-home-asim");
        assert_eq!(
            translate("-home-asim-code-habits", "/home/asim", "/Users/asim").unwrap(),
            "-Users-asim-code-habits"
        );
    }
    #[test]
    fn translate_rejects_other_prefixes() {
        assert_eq!(translate("-Users-asimov-x", "/Users/asim", "/home/asim"), None);
        assert_eq!(translate("-tmp-x", "/Users/asim", "/home/asim"), None);
    }
    #[test]
    fn display_path_is_tilde_relative() {
        assert_eq!(display_path("-Users-asim-wagt", "/Users/asim"), "~/wagt");
        assert_eq!(display_path("-Users-asim", "/Users/asim"), "~");
        assert_eq!(display_path("-Users-asim-Desktop-wagt-ai", "/Users/asim"), "~/Desktop-wagt-ai");
    }
}
