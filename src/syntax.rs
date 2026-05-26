//! Syntax highlighting for text editors.
//!
//! Provides a [`SyntaxHighlighter`] trait and built-in implementations
//! for plain text and Rust source code.

use ratatui::style::{Color, Style};

/// A trait for syntax highlighters that tokenize a line of text
/// into styled segments.
///
/// # Example
///
/// ```ignore
/// use four_turbo_tui::syntax::{SyntaxHighlighter, RustHighlighter};
///
/// let highlighter = RustHighlighter;
/// let tokens = highlighter.highlight("fn hello() {");
/// // tokens: [("fn", blue), (" ", default), ("hello", default), ...]
/// ```
pub trait SyntaxHighlighter {
    /// Tokenize a single line of text into styled segments.
    ///
    /// Returns a vector of `(Style, &str)` pairs. The string slices
    /// reference the original input and are guaranteed to be contiguous
    /// and non-overlapping. Together they cover the entire line.
    fn highlight<'a>(&self, line: &'a str) -> Vec<(Style, &'a str)>;
}

/// A syntax highlighter that returns the entire line with no styling.
///
/// This is the default highlighter for when no specific language is
/// selected. It produces a single segment with the default style.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlainTextHighlighter;

impl SyntaxHighlighter for PlainTextHighlighter {
    fn highlight<'a>(&self, line: &'a str) -> Vec<(Style, &'a str)> {
        if line.is_empty() {
            return Vec::new();
        }
        vec![(Style::default(), line)]
    }
}

/// A basic Rust syntax highlighter.
///
/// Highlights:
/// - Keywords (`fn`, `let`, `mut`, `pub`, `use`, `impl`, `struct`, `enum`,
///   `return`, `if`, `else`, `for`, `while`, `match`, `const`, `static`,
///   `trait`, `type`, `mod`, `where`, `async`, `await`, `unsafe`, `ref`,
///   `move`, `dyn`, `in`, `self`, `super`, `crate`)
/// - String literals (delimited by `"`)
/// - Character literals (delimited by `'`)
/// - Line comments (`// ...`)
/// - Block comments (`/* ... */`)
/// - Numbers (integer and float literals)
/// - Attributes (`#[...]`)
/// - Built-in types (`i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`,
///   `f32`, `f64`, `bool`, `char`, `String`, `Vec`, `Option`, `Result`,
///   `Box`, `Rc`, `Arc`, `usize`, `isize`)
///
/// This is intentionally basic — it does not handle every edge case
/// but provides useful visual distinction for common Rust constructs.
#[derive(Debug, Clone, Copy, Default)]
pub struct RustHighlighter;

impl RustHighlighter {
    /// Check if a word is a Rust keyword.
    fn is_keyword(word: &str) -> bool {
        matches!(
            word,
            "fn"
                | "let"
                | "mut"
                | "pub"
                | "use"
                | "impl"
                | "struct"
                | "enum"
                | "return"
                | "if"
                | "else"
                | "for"
                | "while"
                | "match"
                | "const"
                | "static"
                | "trait"
                | "type"
                | "mod"
                | "where"
                | "async"
                | "await"
                | "unsafe"
                | "ref"
                | "move"
                | "dyn"
                | "in"
                | "self"
                | "super"
                | "crate"
                | "macro_rules"
        )
    }

    /// Check if a word is a Rust built-in type.
    fn is_builtin_type(word: &str) -> bool {
        matches!(
            word,
            "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "f32"
                | "f64"
                | "bool"
                | "char"
                | "String"
                | "Vec"
                | "Option"
                | "Result"
                | "Box"
                | "Rc"
                | "Arc"
                | "usize"
                | "isize"
                | "HashMap"
                | "HashSet"
                | "VecDeque"
                | "Cell"
                | "RefCell"
                | "Mutex"
                | "RwLock"
        )
    }

    /// Try to parse a number literal prefix.
    fn number_len(s: &str) -> usize {
        let mut len = 0;
        let mut has_digit = false;
        for ch in s.chars() {
            let is_digit = ch.is_ascii_digit();
            if is_digit
                || ch == '.'
                || ch == '_'
                || ch == 'x'
                || ch == 'o'
                || ch == 'b'
                || (has_digit && (ch == 'e' || ch == 'E'))
            {
                len += ch.len_utf8();
                if is_digit {
                    has_digit = true;
                }
            } else {
                break;
            }
        }
        len
    }
}

impl SyntaxHighlighter for RustHighlighter {
    #[allow(clippy::too_many_lines)]
    fn highlight<'a>(&self, line: &'a str) -> Vec<(Style, &'a str)> {
        let mut tokens: Vec<(Style, &'a str)> = Vec::new();
        let mut remaining = line;

        while !remaining.is_empty() {
            // Whitespace
            let ws_len = remaining
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
            if ws_len > 0 {
                tokens.push((Style::default(), &remaining[..ws_len]));
                remaining = &remaining[ws_len..];
                continue;
            }

            // Line comment
            if remaining.starts_with("//") {
                tokens.push((Style::default().fg(Color::DarkGray), remaining));
                break;
            }

            // Block comment start
            if remaining.starts_with("/*") {
                if let Some(pos) = remaining[2..].find("*/") {
                    let end_pos = pos + 4;
                    tokens.push((
                        Style::default().fg(Color::DarkGray),
                        &remaining[..end_pos],
                    ));
                    remaining = &remaining[end_pos..];
                } else {
                    tokens.push((Style::default().fg(Color::DarkGray), remaining));
                    break;
                }
                continue;
            }

            // String literal
            if remaining.starts_with('"') {
                let mut end = 1;
                let mut escaped = false;
                for ch in remaining[1..].chars() {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        end += ch.len_utf8();
                        break;
                    }
                    end += ch.len_utf8();
                }
                tokens.push((Style::default().fg(Color::Green), &remaining[..end]));
                remaining = &remaining[end..];
                continue;
            }

            // Raw string literal (r"...", r#"..."#, etc.)
            if remaining.starts_with('r') {
                let after_r = &remaining[1..];
                let mut hash_count = 0;
                for ch in after_r.chars() {
                    if ch == '#' {
                        hash_count += 1;
                    } else {
                        break;
                    }
                }
                if hash_count > 0 || after_r.starts_with('"') {
                    let after_hashes = &remaining[1 + hash_count..];
                    if let Some(rest) = after_hashes.strip_prefix('"') {
                        // Find closing "
                        let close_str = format!("\"{}", "#".repeat(hash_count));
                        if let Some(pos) = rest.find(&close_str) {
                            let end_pos = 1 + hash_count + 1 + pos + close_str.len();
                            tokens.push((
                                Style::default().fg(Color::Green),
                                &remaining[..end_pos],
                            ));
                            remaining = &remaining[end_pos..];
                        } else {
                            tokens.push((Style::default().fg(Color::Green), remaining));
                            break;
                        }
                    } else {
                        // Not a raw string, parse identifier
                        let ident_len = remaining
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .map(char::len_utf8)
                            .sum::<usize>();
                        if ident_len > 0 {
                            let word = &remaining[..ident_len];
                            let style = if Self::is_keyword(word) {
                                Style::default().fg(Color::Blue)
                            } else if Self::is_builtin_type(word) {
                                Style::default().fg(Color::Cyan)
                            } else {
                                Style::default()
                            };
                            tokens.push((style, word));
                            remaining = &remaining[ident_len..];
                        } else {
                            tokens.push((Style::default(), &remaining[..1]));
                            remaining = &remaining[1..];
                        }
                        continue;
                    }
                    continue;
                }
            }

            // Character literal
            if remaining.starts_with('\'') {
                let mut end = 1;
                let mut escaped = false;
                for ch in remaining[1..].chars() {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '\'' {
                        end += ch.len_utf8();
                        break;
                    }
                    end += ch.len_utf8();
                }
                tokens.push((
                    Style::default().fg(Color::Green),
                    &remaining[..end],
                ));
                remaining = &remaining[end..];
                continue;
            }

            // Attribute: #[...]
            if remaining.starts_with("#[") {
                if let Some(pos) = remaining.find(']') {
                    let end_pos = pos + 1;
                    tokens.push((
                        Style::default().fg(Color::DarkGray),
                        &remaining[..end_pos],
                    ));
                    remaining = &remaining[end_pos..];
                } else {
                    tokens.push((Style::default().fg(Color::DarkGray), remaining));
                    break;
                }
                continue;
            }

            // Number literal
            if remaining.starts_with(|c: char| c.is_ascii_digit())
                || remaining.starts_with('.')
                || remaining.starts_with("-.")
                || remaining.starts_with("+.")
            {
                let num_len = Self::number_len(remaining);
                if num_len > 0 {
                    tokens.push((
                        Style::default().fg(Color::Yellow),
                        &remaining[..num_len],
                    ));
                    remaining = &remaining[num_len..];
                    continue;
                }
            }

            // Identifier or keyword
            if remaining.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                let ident_len = remaining
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .map(char::len_utf8)
                    .sum::<usize>();
                if ident_len > 0 {
                    let word = &remaining[..ident_len];
                    let style = if Self::is_keyword(word) {
                        Style::default().fg(Color::Blue)
                    } else if Self::is_builtin_type(word) {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    tokens.push((style, word));
                    remaining = &remaining[ident_len..];
                    continue;
                }
            }

            // Punctuation / other single characters
            let ch_len = remaining
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            tokens.push((Style::default(), &remaining[..ch_len]));
            remaining = &remaining[ch_len..];
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_highlighter() {
        let h = PlainTextHighlighter;
        let tokens = h.highlight("hello world");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].1, "hello world");
    }

    #[test]
    fn test_plain_text_empty_line() {
        let h = PlainTextHighlighter;
        let tokens = h.highlight("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_rust_keyword_highlighting() {
        let h = RustHighlighter;
        let tokens = h.highlight("fn main() {");

        // "fn" should be blue (keyword)
        let fn_token = tokens.iter().find(|(_, s)| *s == "fn");
        assert!(fn_token.is_some(), "Should find 'fn' token");
        assert_eq!(fn_token.unwrap().0.fg, Some(Color::Blue));
    }

    #[test]
    fn test_rust_string_highlighting() {
        let h = RustHighlighter;
        let tokens = h.highlight(r#"let s = "hello";"#);

        // "hello" with quotes should be green
        let str_token = tokens.iter().find(|(_, s)| *s == "\"hello\"");
        assert!(str_token.is_some(), "Should find string literal token");
        assert_eq!(str_token.unwrap().0.fg, Some(Color::Green));
    }

    #[test]
    fn test_rust_comment_highlighting() {
        let h = RustHighlighter;
        let tokens = h.highlight("// this is a comment");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0.fg, Some(Color::DarkGray));
    }

    #[test]
    fn test_rust_block_comment() {
        let h = RustHighlighter;
        let tokens = h.highlight("/* block */ code");

        // First token should be the block comment
        assert_eq!(tokens[0].0.fg, Some(Color::DarkGray));
        assert_eq!(tokens[0].1, "/* block */");
    }

    #[test]
    fn test_rust_number_highlighting() {
        let h = RustHighlighter;
        let tokens = h.highlight("let x = 42;");

        let num_token = tokens.iter().find(|(_, s)| *s == "42");
        assert!(num_token.is_some(), "Should find '42' token");
        assert_eq!(num_token.unwrap().0.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_rust_builtin_type() {
        let h = RustHighlighter;
        let tokens = h.highlight("let x: i32 = 5;");

        let type_token = tokens.iter().find(|(_, s)| *s == "i32");
        assert!(type_token.is_some(), "Should find 'i32' token");
        assert_eq!(type_token.unwrap().0.fg, Some(Color::Cyan));
    }

    #[test]
    fn test_rust_attribute() {
        let h = RustHighlighter;
        let tokens = h.highlight("#[derive(Debug)]");

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0.fg, Some(Color::DarkGray));
    }
}
