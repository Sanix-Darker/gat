//! Shared error model for the `gat` binary.
//!
//! The CLI is intentionally small, so a single domain error enum keeps error
//! handling explicit without pulling in a general-purpose error crate.

use std::fmt::{Display, Formatter};

/// Error type used across command parsing, Git interaction, and filesystem
/// operations.
///
/// Variants are user-facing: [`Display`] output should remain concise and
/// actionable because it is printed directly by `main`.
#[derive(Debug)]
pub enum GatError {
    /// The user provided an invalid command or option combination.
    Usage(String),
    /// A Git subprocess returned a non-zero status.
    Git { command: String, message: String },
    /// A filesystem or subprocess spawning operation failed.
    Io(String),
    /// The command needs a Git worktree but was run outside one.
    NotGitRepo,
    /// A requested worktree, branch, binary, or external tool was not found.
    NotFound(String),
    /// The requested operation may lose work unless forced explicitly.
    Unsafe(String),
}

impl Display for GatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "usage error: {message}"),
            Self::Git { command, message } => {
                write!(f, "git command failed: {command}\n{message}")
            }
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::NotGitRepo => write!(f, "not inside a Git worktree"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Unsafe(message) => write!(f, "refusing unsafe operation: {message}"),
        }
    }
}

impl std::error::Error for GatError {}

impl From<std::io::Error> for GatError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

/// Convenient result alias used by all modules.
pub type Result<T> = std::result::Result<T, GatError>;
