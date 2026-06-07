//! Output formatting helpers.
//!
//! Commands can be read by humans, shell wrappers, or other tools. This module
//! centralizes the small amount of escaping needed to keep those formats safe
//! without depending on a serialization crate.

use std::path::Path;

/// Output mode requested by a command.
///
/// `Shell` is deliberately separate from `Text`: it emits assignment lines that
/// the shell integration can evaluate after the Rust process exits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    /// Human-readable text.
    Text,
    /// Machine-readable JSON.
    Json,
    /// POSIX-shell assignment output for `gat shell-init`.
    Shell,
}

impl OutputFormat {
    /// Parses the value accepted by `--format`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }
}

/// Escapes a string for insertion into manually formatted JSON.
///
/// The project currently avoids external dependencies; this function covers the
/// control characters and quoting needed by the values `gat` emits.
pub fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Quotes a string as a single POSIX shell word.
///
/// The generated shell integration uses this for paths and messages before
/// passing assignment output to `eval`.
pub fn shell_escape(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Converts a path to display text without panicking on non-UTF-8 paths.
pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_parses_known_values() {
        assert_eq!(OutputFormat::parse("text"), Some(OutputFormat::Text));
        assert_eq!(OutputFormat::parse("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("shell"), Some(OutputFormat::Shell));
        assert_eq!(OutputFormat::parse("yaml"), None);
        assert_eq!(OutputFormat::parse(""), None);
    }

    #[test]
    fn json_escape_handles_quotes_and_controls() {
        assert_eq!(json_escape("plain"), "plain");
        assert_eq!(json_escape("a\"b"), "a\\\"b");
        assert_eq!(json_escape("a\\b"), "a\\\\b");
        assert_eq!(json_escape("line1\nline2"), "line1\\nline2");
        assert_eq!(json_escape("tab\there"), "tab\\there");
        // A control character (bell) becomes a \u escape.
        assert_eq!(json_escape("\u{7}"), "\\u0007");
    }

    #[test]
    fn shell_escape_quotes_and_escapes_single_quotes() {
        assert_eq!(shell_escape(""), "''");
        assert_eq!(shell_escape("plain"), "'plain'");
        assert_eq!(shell_escape("with space"), "'with space'");
        // Embedded single quotes use the close-escape-reopen idiom.
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        // Shell metacharacters are safely contained inside single quotes.
        assert_eq!(shell_escape("a;b|c"), "'a;b|c'");
    }

    #[test]
    fn path_string_renders_paths() {
        assert_eq!(path_string(Path::new("/a/b")), "/a/b");
        assert_eq!(path_string(Path::new("rel/path")), "rel/path");
    }
}
