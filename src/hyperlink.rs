//! OSC 8 hyperlink support: capability detection and URL sanitisation.
//!
//! Terminals advertise OSC 8 hyperlink support inconsistently. This module turns
//! an already-collected `CSI > 0 q` (XTVERSION) answer plus a set of environment
//! variables into a [`HyperlinkSupport`] decision, and sanitises the URLs that
//! [`Terminal`](crate::terminal::Terminal) may emit as `ESC]8;;…ESC\` sequences.
//!
//! # Boundary: no active terminal I/O
//!
//! Detection here never writes a query to the terminal and never reads from it.
//! The active XTVERSION I/O round — writing `CSI > 0 q` and waiting for the
//! answer with a timeout — belongs to the shared startup query round tracked in
//! #61, because reading the reply directly would steal crossterm events.
//! [`detect`] therefore consumes an `Option<&str>` answer that the caller has
//! already collected, which keeps the logic pure and fully testable with fake
//! answers and environment closures.
//!
//! # Safe default
//!
//! When nothing identifies a known-capable terminal the result is disabled.
//! Hyperlinks are off unless detection (or an explicit override) says otherwise.

use crate::core::{Link, LinkTarget};

/// Schemes allowed for OSC 8 emission when none are configured explicitly.
pub const DEFAULT_LINK_SCHEMES: [&str; 3] = ["http", "https", "file"];

/// How hyperlink emission is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HyperlinkMode {
    /// Detect support from the environment and, if present, the XTVERSION
    /// answer.
    #[default]
    Auto,
    /// Force hyperlinks on regardless of detection.
    On,
    /// Force hyperlinks off regardless of detection.
    Off,
}

impl HyperlinkMode {
    /// Reads `TURBO_TUI_HYPERLINKS`.
    ///
    /// `on`/`true`/`1` map to [`HyperlinkMode::On`], `off`/`false`/`0` map to
    /// [`HyperlinkMode::Off`]; every other value (including an absent variable)
    /// maps to [`HyperlinkMode::Auto`].
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("TURBO_TUI_HYPERLINKS") {
            Ok(value) => match value.to_ascii_lowercase().as_str() {
                "on" | "true" | "1" => Self::On,
                "off" | "false" | "0" => Self::Off,
                _ => Self::Auto,
            },
            Err(_) => Self::Auto,
        }
    }
}

/// Result of hyperlink capability detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HyperlinkSupport {
    /// Whether OSC 8 hyperlinks may be emitted.
    pub enabled: bool,
    /// Human-readable explanation of the decision.
    pub reason: String,
}

/// Detects OSC 8 hyperlink support from a mode, an optional XTVERSION answer
/// and an environment lookup.
///
/// `xtversion` is the answer to `CSI > 0 q` if the startup query already ran;
/// `env` abstracts the process environment so the logic stays pure and testable.
/// No terminal I/O happens here — see the module docs.
///
/// Precedence: explicit mode, then `TURBO_TUI_HYPERLINKS`, then `tmux` and GNU
/// screen guards, then the XTVERSION answer, then environment fallbacks, and
/// finally an off default.
#[must_use]
pub fn detect(
    mode: HyperlinkMode,
    xtversion: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> HyperlinkSupport {
    match mode {
        HyperlinkMode::Off => return support(false, "disabled by mode"),
        HyperlinkMode::On => return support(true, "forced on"),
        HyperlinkMode::Auto => {}
    }

    if let Some(value) = env("TURBO_TUI_HYPERLINKS") {
        match value.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => return support(true, "forced by TURBO_TUI_HYPERLINKS"),
            "off" | "false" | "0" => {
                return support(false, "disabled by TURBO_TUI_HYPERLINKS");
            }
            "auto" | "" => {}
            _ => {
                return HyperlinkSupport {
                    enabled: false,
                    reason: format!("invalid TURBO_TUI_HYPERLINKS value: {value}"),
                };
            }
        }
    }

    if env("TMUX").is_some() {
        return support(false, "tmux: hyperlink support unverified");
    }

    if env("TERM").is_some_and(|term| term.to_ascii_lowercase().starts_with("screen")) {
        return support(false, "GNU screen: no OSC 8 support");
    }

    if let Some(answer) = xtversion {
        if let Some(result) = from_xtversion(answer) {
            return result;
        }
    }

    from_env_fallback(env).unwrap_or_else(|| support(false, "unknown terminal"))
}

/// Maps a known XTVERSION answer to a support decision, or `None` when the
/// terminal is unknown or the answer is unparsable.
fn from_xtversion(answer: &str) -> Option<HyperlinkSupport> {
    let lower = answer.to_ascii_lowercase();
    if lower.contains("wezterm") {
        return Some(support(true, "XTVERSION: WezTerm"));
    }
    if lower.contains("kitty") {
        return Some(support(true, "XTVERSION: kitty"));
    }
    if lower.contains("iterm") {
        return Some(support(true, "XTVERSION: iTerm2"));
    }
    if lower.contains("vte(") {
        if let Some(version) = parse_first_paren_u32(answer) {
            return Some(if version >= 5000 {
                support(true, "XTVERSION: VTE >= 0.50")
            } else {
                support(false, "XTVERSION: VTE too old for OSC 8")
            });
        }
    }
    if lower.contains("konsole") {
        return Some(support(true, "XTVERSION: Konsole"));
    }
    if lower.contains("foot(") {
        return Some(support(true, "XTVERSION: foot"));
    }
    if lower.contains("alacritty") {
        return Some(support(true, "XTVERSION: Alacritty"));
    }
    if lower.contains("windows terminal") {
        return Some(support(true, "XTVERSION: Windows Terminal"));
    }
    None
}

/// Maps environment fallbacks to a support decision, or `None` when nothing
/// identifies a known-capable terminal.
fn from_env_fallback(env: &dyn Fn(&str) -> Option<String>) -> Option<HyperlinkSupport> {
    if env("WT_SESSION").is_some() {
        return Some(support(true, "Windows Terminal"));
    }
    if env("KITTY_WINDOW_ID").is_some() {
        return Some(support(true, "kitty"));
    }
    if let Some(value) = env("VTE_VERSION") {
        if let Ok(version) = value.trim().parse::<u32>() {
            return Some(if version >= 5000 {
                support(true, "VTE >= 0.50")
            } else {
                support(false, "VTE too old for OSC 8")
            });
        }
    }
    if let Some(value) = env("KONSOLE_VERSION") {
        if let Ok(version) = value.trim().parse::<u32>() {
            return Some(if version >= 200_800 {
                support(true, "Konsole >= 20.08")
            } else {
                support(false, "Konsole too old for OSC 8")
            });
        }
    }
    if let Some(program) = env("TERM_PROGRAM") {
        match program.to_ascii_lowercase().as_str() {
            "wezterm" => return Some(support(true, "WezTerm")),
            "iterm.app" => return Some(support(true, "iTerm2")),
            "vscode" => return Some(support(true, "VS Code")),
            "hyper" => return Some(support(true, "Hyper")),
            "apple_terminal" => {
                return Some(support(false, "Apple Terminal: no OSC 8 support"));
            }
            _ => {}
        }
    }
    if let Some(term) = env("TERM") {
        let lower = term.to_ascii_lowercase();
        if lower.starts_with("xterm-kitty") {
            return Some(support(true, "kitty (TERM)"));
        }
        if lower.contains("wezterm") {
            return Some(support(true, "WezTerm (TERM)"));
        }
        if lower.contains("foot") {
            return Some(support(true, "foot (TERM)"));
        }
        if lower.contains("alacritty") {
            return Some(support(true, "Alacritty (TERM)"));
        }
    }
    None
}

/// Detects support against the real process environment.
///
/// Equivalent to [`detect`] with a lookup into [`std::env::var`]. It performs no
/// terminal I/O; `xtversion` is still the answer supplied by the caller.
#[must_use]
pub fn detect_current(mode: HyperlinkMode, xtversion: Option<&str>) -> HyperlinkSupport {
    detect(mode, xtversion, &|key| std::env::var(key).ok())
}

/// Sanitises a [`LinkTarget::Url`] for OSC 8 emission.
///
/// All control characters (C0, C1 and DEL) are stripped. Returns `None` for
/// non-URL targets, malformed or empty schemes, empty payloads, and schemes not
/// in `allowed_schemes`. Scheme comparison is ASCII case-insensitive; the
/// returned string keeps the URL's original casing.
#[must_use]
pub fn sanitized_url(link: &Link, allowed_schemes: &[String]) -> Option<String> {
    let LinkTarget::Url(url) = &link.target else {
        return None;
    };

    let cleaned: String = url.chars().filter(|ch| !ch.is_control()).collect();
    let (scheme, payload) = cleaned.split_once(':')?;

    if scheme.is_empty() || payload.is_empty() || !is_valid_scheme(scheme) {
        return None;
    }
    if !allowed_schemes
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(scheme))
    {
        return None;
    }

    Some(cleaned)
}

/// Builds a [`HyperlinkSupport`] from a boolean and a reason string.
fn support(enabled: bool, reason: &str) -> HyperlinkSupport {
    HyperlinkSupport {
        enabled,
        reason: reason.to_owned(),
    }
}

/// Parses the first unsigned integer following the first `(` in `text`.
///
/// This reads the version out of an XTVERSION answer such as `VTE(5000)`.
fn parse_first_paren_u32(text: &str) -> Option<u32> {
    let start = text.find('(')? + 1;
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Returns `true` when `scheme` matches RFC 3986: an ASCII letter followed by
/// ASCII letters, digits, `+`, `-` or `.`.
fn is_valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Runs [`detect`] with a fake environment built from `pairs`.
    fn detect_with(
        pairs: &[(&str, &str)],
        mode: HyperlinkMode,
        xtversion: Option<&str>,
    ) -> HyperlinkSupport {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        let env = |key: &str| map.get(key).cloned();
        detect(mode, xtversion, &env)
    }

    fn default_schemes() -> Vec<String> {
        DEFAULT_LINK_SCHEMES
            .iter()
            .copied()
            .map(str::to_owned)
            .collect()
    }

    fn url_link(url: &str) -> Link {
        Link::new(LinkTarget::Url(url.to_owned()))
    }

    // -- mode ---------------------------------------------------------------

    #[test]
    fn test_detect_off_mode() {
        let support = detect_with(&[], HyperlinkMode::Off, None);
        assert!(!support.enabled);
        assert_eq!(support.reason, "disabled by mode");
    }

    #[test]
    fn test_detect_on_mode() {
        let support = detect_with(&[], HyperlinkMode::On, None);
        assert!(support.enabled);
        assert_eq!(support.reason, "forced on");
    }

    // -- TURBO_TUI_HYPERLINKS override --------------------------------------

    #[test]
    fn test_env_override_on() {
        let support = detect_with(&[("TURBO_TUI_HYPERLINKS", "1")], HyperlinkMode::Auto, None);
        assert!(support.enabled);
        assert_eq!(support.reason, "forced by TURBO_TUI_HYPERLINKS");
    }

    #[test]
    fn test_env_override_off() {
        let support = detect_with(
            &[("TURBO_TUI_HYPERLINKS", "false")],
            HyperlinkMode::Auto,
            None,
        );
        assert!(!support.enabled);
        assert_eq!(support.reason, "disabled by TURBO_TUI_HYPERLINKS");
    }

    #[test]
    fn test_env_override_invalid() {
        let support = detect_with(
            &[("TURBO_TUI_HYPERLINKS", "maybe")],
            HyperlinkMode::Auto,
            None,
        );
        assert!(!support.enabled);
        assert_eq!(support.reason, "invalid TURBO_TUI_HYPERLINKS value: maybe");
    }

    #[test]
    fn test_env_auto_continues_detection() {
        let support = detect_with(
            &[("TURBO_TUI_HYPERLINKS", "auto"), ("WT_SESSION", "1")],
            HyperlinkMode::Auto,
            None,
        );
        assert!(support.enabled);
        assert_eq!(support.reason, "Windows Terminal");
    }

    // -- tmux and screen guards ---------------------------------------------

    #[test]
    fn test_tmux_disables() {
        let support = detect_with(&[("TMUX", "/tmp/tmux")], HyperlinkMode::Auto, None);
        assert!(!support.enabled);
        assert_eq!(support.reason, "tmux: hyperlink support unverified");
    }

    #[test]
    fn test_screen_disables() {
        let support = detect_with(&[("TERM", "screen-256color")], HyperlinkMode::Auto, None);
        assert!(!support.enabled);
        assert_eq!(support.reason, "GNU screen: no OSC 8 support");
    }

    #[test]
    fn test_tmux_wins_over_vte_kitty_and_screen() {
        let support = detect_with(
            &[
                ("TMUX", "/tmp/tmux"),
                ("TERM", "screen-256color"),
                ("VTE_VERSION", "5000"),
                ("KITTY_WINDOW_ID", "1"),
            ],
            HyperlinkMode::Auto,
            None,
        );
        assert!(!support.enabled);
        assert_eq!(support.reason, "tmux: hyperlink support unverified");
    }

    // -- XTVERSION ----------------------------------------------------------

    #[test]
    fn test_xtversion_known_terminals() {
        let cases = [
            ("WezTerm 20240203", "XTVERSION: WezTerm"),
            ("kitty(0.32.0)", "XTVERSION: kitty"),
            ("iTerm2 3.5.0", "XTVERSION: iTerm2"),
            ("Konsole 22.04", "XTVERSION: Konsole"),
            ("foot(1.16.2)", "XTVERSION: foot"),
            ("Alacritty 0.13", "XTVERSION: Alacritty"),
            ("Windows Terminal(1.18)", "XTVERSION: Windows Terminal"),
        ];
        for (answer, expected) in cases {
            let support = detect_with(&[], HyperlinkMode::Auto, Some(answer));
            assert!(support.enabled, "expected support for {answer:?}");
            assert_eq!(support.reason, expected);
        }
    }

    #[test]
    fn test_xtversion_vte_new_enough() {
        let support = detect_with(&[], HyperlinkMode::Auto, Some("VTE(5000)"));
        assert!(support.enabled);
        assert_eq!(support.reason, "XTVERSION: VTE >= 0.50");
    }

    #[test]
    fn test_xtversion_vte_too_old() {
        let support = detect_with(&[], HyperlinkMode::Auto, Some("VTE(4999)"));
        assert!(!support.enabled);
        assert_eq!(support.reason, "XTVERSION: VTE too old for OSC 8");
    }

    #[test]
    fn test_xtversion_vte_unparsable_falls_through() {
        let support = detect_with(
            &[("WT_SESSION", "1")],
            HyperlinkMode::Auto,
            Some("VTE(nope)"),
        );
        assert!(support.enabled);
        assert_eq!(support.reason, "Windows Terminal");
    }

    #[test]
    fn test_xtversion_unknown_falls_through_to_env() {
        let support = detect_with(
            &[("WT_SESSION", "1")],
            HyperlinkMode::Auto,
            Some("SomeUnknownTerminal"),
        );
        assert!(support.enabled);
        assert_eq!(support.reason, "Windows Terminal");
    }

    // -- environment fallback ----------------------------------------------

    #[test]
    fn test_env_wt_session() {
        let support = detect_with(&[("WT_SESSION", "abc")], HyperlinkMode::Auto, None);
        assert!(support.enabled);
        assert_eq!(support.reason, "Windows Terminal");
    }

    #[test]
    fn test_env_kitty_window_id() {
        let support = detect_with(&[("KITTY_WINDOW_ID", "1")], HyperlinkMode::Auto, None);
        assert!(support.enabled);
        assert_eq!(support.reason, "kitty");
    }

    #[test]
    fn test_env_vte_version_gate() {
        let new = detect_with(&[("VTE_VERSION", "5000")], HyperlinkMode::Auto, None);
        assert!(new.enabled);
        assert_eq!(new.reason, "VTE >= 0.50");

        let old = detect_with(&[("VTE_VERSION", "4999")], HyperlinkMode::Auto, None);
        assert!(!old.enabled);
        assert_eq!(old.reason, "VTE too old for OSC 8");
    }

    #[test]
    fn test_env_konsole_version_gate() {
        let new = detect_with(&[("KONSOLE_VERSION", "200800")], HyperlinkMode::Auto, None);
        assert!(new.enabled);
        assert_eq!(new.reason, "Konsole >= 20.08");

        let old = detect_with(&[("KONSOLE_VERSION", "200700")], HyperlinkMode::Auto, None);
        assert!(!old.enabled);
        assert_eq!(old.reason, "Konsole too old for OSC 8");
    }

    #[test]
    fn test_env_term_program_variants() {
        let cases = [
            ("WezTerm", true, "WezTerm"),
            ("iTerm.app", true, "iTerm2"),
            ("vscode", true, "VS Code"),
            ("Hyper", true, "Hyper"),
            ("Apple_Terminal", false, "Apple Terminal: no OSC 8 support"),
        ];
        for (program, enabled, reason) in cases {
            let support = detect_with(&[("TERM_PROGRAM", program)], HyperlinkMode::Auto, None);
            assert_eq!(support.enabled, enabled, "TERM_PROGRAM={program}");
            assert_eq!(support.reason, reason);
        }
    }

    #[test]
    fn test_env_term_variants() {
        let cases = [
            ("xterm-kitty", "kitty (TERM)"),
            ("wezterm", "WezTerm (TERM)"),
            ("foot", "foot (TERM)"),
            ("alacritty", "Alacritty (TERM)"),
        ];
        for (term, reason) in cases {
            let support = detect_with(&[("TERM", term)], HyperlinkMode::Auto, None);
            assert!(support.enabled, "TERM={term}");
            assert_eq!(support.reason, reason);
        }
    }

    #[test]
    fn test_unknown_terminal_is_off() {
        let support = detect_with(&[], HyperlinkMode::Auto, None);
        assert!(!support.enabled);
        assert_eq!(support.reason, "unknown terminal");
    }

    // -- sanitized_url ------------------------------------------------------

    #[test]
    fn test_sanitized_url_strips_control_chars() {
        let link = url_link("https://example.com/\x1b\x07\u{9b}\u{7f}path");
        assert_eq!(
            sanitized_url(&link, &default_schemes()),
            Some("https://example.com/path".to_owned())
        );
    }

    #[test]
    fn test_sanitized_url_keeps_valid_https() {
        let link = url_link("https://example.com/path?q=1#frag");
        assert_eq!(
            sanitized_url(&link, &default_schemes()),
            Some("https://example.com/path?q=1#frag".to_owned())
        );
    }

    #[test]
    fn test_sanitized_url_allows_file() {
        let link = url_link("file:///tmp/notes.txt");
        assert_eq!(
            sanitized_url(&link, &default_schemes()),
            Some("file:///tmp/notes.txt".to_owned())
        );
    }

    #[test]
    fn test_sanitized_url_rejects_javascript_by_default() {
        let link = url_link("javascript:alert(1)");
        assert_eq!(sanitized_url(&link, &default_schemes()), None);
    }

    #[test]
    fn test_sanitized_url_allows_configured_scheme() {
        let link = url_link("ftp://ftp.example.com/file");
        let schemes = vec!["ftp".to_owned()];
        assert_eq!(
            sanitized_url(&link, &schemes),
            Some("ftp://ftp.example.com/file".to_owned())
        );
    }

    #[test]
    fn test_sanitized_url_rejects_non_url_targets() {
        let issue = Link::new(LinkTarget::IssueRef {
            repo: Some("four-bytes/turbo-tui".to_owned()),
            number: 58,
        });
        let file_pos = Link::new(LinkTarget::FilePos {
            path: "src/lib.rs".to_owned(),
            line: 1,
            col: Some(2),
        });
        let command = Link::new(LinkTarget::Command(crate::command::CM_QUIT, None));
        for link in [issue, file_pos, command] {
            assert_eq!(sanitized_url(&link, &default_schemes()), None);
        }
    }

    #[test]
    fn test_sanitized_url_rejects_empty_scheme_or_payload() {
        assert_eq!(sanitized_url(&url_link(":foo"), &default_schemes()), None);
        assert_eq!(sanitized_url(&url_link("https:"), &default_schemes()), None);
    }

    #[test]
    fn test_sanitized_url_rejects_malformed_scheme() {
        assert_eq!(
            sanitized_url(&url_link("ht tp://example.com"), &default_schemes()),
            None
        );
        assert_eq!(
            sanitized_url(&url_link("1http://example.com"), &default_schemes()),
            None
        );
    }

    #[test]
    fn test_sanitized_url_accepts_uppercase_scheme() {
        let link = url_link("HTTPS://Example.com/Path");
        assert_eq!(
            sanitized_url(&link, &default_schemes()),
            Some("HTTPS://Example.com/Path".to_owned())
        );
    }
}
