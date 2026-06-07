//! `gat` is a ticket-oriented Git worktree helper.
//!
//! The binary keeps Git as the source of truth and layers a small, fast command
//! interface on top of native `git worktree`, `tmux`, `fzf`, and shell
//! integration. The public command surface is intentionally implemented with
//! the standard library only so startup remains cheap and installation is just
//! `cargo install --path .`.

#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

mod app;
mod cli;
mod config;
mod docker;
mod error;
mod git;
mod metadata;
mod output;
#[cfg(feature = "tui")]
mod tui;

use std::process::ExitCode;

/// Parses command-line arguments, delegates to the application layer, and maps
/// domain failures to process exit codes.
///
/// Exit code `0` means the command completed. Exit code `1` is a runtime or
/// Git failure. Exit code `2` is reserved for CLI usage errors.
fn main() -> ExitCode {
    // Initialize logging based on GAT_VERBOSE or RUST_LOG
    if std::env::var("GAT_VERBOSE").is_ok() || std::env::var("RUST_LOG").is_ok() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    match cli::parse(std::env::args().skip(1).collect()) {
        Ok(command) => match app::run(command) {
            Ok(result) => {
                print!("{result}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::from(1)
            }
        },
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(2)
        }
    }
}
