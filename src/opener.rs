//! Platform opener for external links.
//!
//! [`open_url`] validates a URL and launches the operating system's default
//! handler without a shell. It is only ever given `http` and `https` URLs, so
//! `file:`, `data:`, `javascript:` and similar schemes are rejected before any
//! process is spawned.

use crate::core::{Link, LinkTarget};
use crate::hyperlink::sanitized_url;
use std::io;
use std::process::Command;

/// Schemes that [`open_url`] is willing to hand to the operating system.
const ALLOWED_SCHEMES: [&str; 2] = ["http", "https"];

/// Returns the program and arguments used to open `url`.
///
/// `os` is one of `"macos"`, `"windows"` or anything else (treated as Linux).
/// When `wsl` is `true`, `wslview` is used regardless of `os`.
#[must_use]
pub fn opener_command(url: &str, os: &str, wsl: bool) -> (&'static str, Vec<String>) {
    if wsl {
        return ("wslview", vec![url.to_owned()]);
    }

    match os {
        "macos" => ("open", vec![url.to_owned()]),
        "windows" => (
            "cmd",
            vec![
                "/C".to_owned(),
                "start".to_owned(),
                String::new(),
                url.to_owned(),
            ],
        ),
        _ => ("xdg-open", vec![url.to_owned()]),
    }
}

/// Validates a URL for opening: strips control characters and accepts only
/// `http`/`https` with a non-empty payload.
///
/// Scheme comparison is ASCII case-insensitive; the returned string keeps the
/// original casing. Returns `None` for any other scheme or an empty payload.
#[must_use]
pub fn validated_url(url: &str) -> Option<String> {
    let allowed: Vec<String> = ALLOWED_SCHEMES.iter().map(|s| (*s).to_owned()).collect();
    let link = Link::new(LinkTarget::Url(url.to_owned()));
    sanitized_url(&link, &allowed)
}

/// Opens `url` with the platform's default handler.
///
/// The URL is validated first; unsupported or unsafe schemes yield
/// [`io::ErrorKind::InvalidInput`]. On WSL, a failing `wslview` falls back to
/// `explorer.exe`. No shell is involved and the spawned process is not waited
/// for.
///
/// # Errors
///
/// Returns the spawn error when the platform opener cannot be started.
pub fn open_url(url: &str) -> io::Result<()> {
    let url = validated_url(url).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to open a non-http(s) or unsafe URL",
        )
    })?;

    let wsl = is_wsl();
    let (program, args) = opener_command(&url, current_os(), wsl);

    match Command::new(program).args(&args).spawn() {
        Ok(_) => Ok(()),
        Err(error) if wsl => Command::new("explorer.exe")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|_| error),
        Err(error) => Err(error),
    }
}

/// Returns `true` when running inside the Windows Subsystem for Linux.
fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some()
}

/// Returns the compile-time operating-system family for [`opener_command`].
fn current_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opener_command_linux() {
        let (program, args) = opener_command("https://example.com", "linux", false);
        assert_eq!(program, "xdg-open");
        assert_eq!(args, vec!["https://example.com"]);
    }

    #[test]
    fn test_opener_command_macos() {
        let (program, args) = opener_command("https://example.com", "macos", false);
        assert_eq!(program, "open");
        assert_eq!(args, vec!["https://example.com"]);
    }

    #[test]
    fn test_opener_command_windows() {
        let (program, args) = opener_command("https://example.com", "windows", false);
        assert_eq!(program, "cmd");
        assert_eq!(args, vec!["/C", "start", "", "https://example.com"]);
    }

    #[test]
    fn test_opener_command_wsl_overrides_os() {
        let (program, args) = opener_command("https://example.com", "windows", true);
        assert_eq!(program, "wslview");
        assert_eq!(args, vec!["https://example.com"]);
    }

    #[test]
    fn test_validated_url_allows_http_and_https() {
        assert_eq!(
            validated_url("http://example.com"),
            Some("http://example.com".to_owned())
        );
        assert_eq!(
            validated_url("https://example.com/path"),
            Some("https://example.com/path".to_owned())
        );
        assert_eq!(
            validated_url("HTTPS://example.com"),
            Some("HTTPS://example.com".to_owned())
        );
    }

    #[test]
    fn test_validated_url_strips_control_characters() {
        assert_eq!(
            validated_url("https://example.com/\u{1b}\u{7}path"),
            Some("https://example.com/path".to_owned())
        );
    }

    #[test]
    fn test_validated_url_rejects_unsafe_and_empty() {
        assert!(validated_url("file:///etc/passwd").is_none());
        assert!(validated_url("data:text/plain,hello").is_none());
        assert!(validated_url("javascript:alert(1)").is_none());
        assert!(validated_url("").is_none());
        assert!(validated_url("https:").is_none());
        assert!(validated_url("example.com").is_none());
        assert!(validated_url(" https://example.com").is_none());
    }

    #[test]
    fn test_open_url_rejects_unsafe_without_spawning() {
        for url in [
            "file:///etc/passwd",
            "data:text/plain,hello",
            "javascript:alert(1)",
            "",
        ] {
            let error = open_url(url).expect_err("unsafe URL must be rejected");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }
}
