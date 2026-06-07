# Changelog

All notable changes to `gat` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-06-04

### Added
- **`gat switch <ticket>`**: Attach to (or open) a worktree's tmux session.
  Reattaches to a live session, otherwise builds the layout for an existing
  worktree, otherwise errors. Never creates a worktree.
- **Per-worktree descriptions**: `gat new --description`, a `gat describe`
  command to set/show/clear, shown in `gat list` (text and JSON) and folded
  into tmux session names (truncated to 100 chars).
- **`gat sessions`**: List live gat-managed tmux sessions with attach state,
  window count, branch, path, and description (text and JSON).
- **Interactive TUI (`gat ui`)**: Keyboard-driven dashboard (worktrees and
  sessions tabs, navigation, incremental filter, switch, describe, remove),
  gated behind an optional `tui` Cargo feature so the default build stays lean.
- **`gat config`**: Inspect and edit the config file via `init`, `path`,
  `list`, `get`, and `set`, with value validation and a TOML writer.
- **`gat merge <ticket>`**: Merge a ticket branch into the default branch from
  the primary worktree with strict safety checks, plus optional cleanup
  (`--remove`, `--delete-branch`, `--kill-session`, `--cleanup`).
- **Worktree setup templates**: `[template.<name>]` config sections that copy
  files, symlink shared directories, and run setup commands after `gat new`;
  selected with `--template`/`--no-template`.
- **Named tmux layout presets**: `classic`, `ai-focus`, `editor-focus`, `wide`,
  selectable per-invocation (`--layout`), via git config (`gat.tmuxLayout`),
  the `GAT_TMUX_LAYOUT` environment variable, or the config file.
- **Change stats in `gat list`**: A `Changes` column (files, insertions,
  deletions) and an `Idle` column (days since last access), with matching JSON
  fields. Per-worktree status is computed in parallel.
- **`gat --version`**: Print the program version.
- **Progress logging**: Mutating commands print `gat:`-prefixed milestones to
  stderr; silence with `GAT_QUIET`.
- **Documentation**: A complete command reference (`docs/COMMANDS.md`) and a man
  page for `gat` and every subcommand (`docs/man/man1/`, installable via
  `make install-man`).
- **Continuous integration**: GitHub Actions running fmt, clippy, and tests for
  both the default and `tui` feature configurations.

### Changed
- Worktree usage metadata now tracks access across `new`, `go`, `switch`,
  `tmux`, `archive`, and is cleaned up on `rm`/`prune`, with atomic writes.
- `gat prune` gained `--older-than <days>` (and `--force`) for age-based
  cleanup; `gat doctor` warns about stale worktrees and prunes orphaned
  metadata.

## [0.2.0] - 2026-06-02

### Added
- **Configurable Tmux Layout**: Full customization of tmux pane sizes, shell, and commands
  - Configure via git config (`gat.tmuxLeftWidth`, `gat.tmuxBottomHeight`, etc.)
  - Configure via environment variables (`GAT_TMUX_*`)
  - Configure via config file (`~/.config/gat/config.toml`)
  - Dynamic shell detection (uses `$SHELL` instead of hardcoded `/bin/bash`)
  - Configurable left pane width (default: 55%)
  - Configurable bottom pane height (default: 35%)
  - Configurable focus pane on creation
- **Advanced Tmux Layout System**: Robust, parsable layout engine with validation
  - Preset layouts: Classic (default), AI-Focus, Editor-Focus, Side-by-Side
  - Strongly-typed layout definitions with serde serialization
  - Comprehensive validation (circular dependencies, duplicate IDs, valid percentages)
  - Variable substitution: `{codex_cmd}`, `{editor_cmd}`, `{prompt_file}`, `{worktree}`
  - Extensible architecture for custom 4+ pane layouts
  - 11 unit tests for layout validation logic
- **Configuration System**: Hierarchical config loading
  - Environment variables (highest priority)
  - Repository git config
  - Global git config
  - Config file (`~/.config/gat/config.toml`)
  - Sensible defaults (lowest priority)
- **Logging Framework**: Structured logging with `log` and `env_logger`
  - Enable with `GAT_VERBOSE=1` or `RUST_LOG=debug`
  - Debug-level logging for troubleshooting
  - Info-level logging for normal operation
- **Improved Docker YAML Parsing**: Uses `serde_yaml` for robust parsing
  - Handles complex YAML structures
  - Supports different indentation styles
  - Falls back to simple parser for edge cases
  - Better error messages
- **Archive Directory Validation**: Early validation of archive paths
  - Checks if directory exists and is writable
  - Verifies parent directory exists
  - Prevents archiving to inappropriate locations
  - Test write permissions before moving worktrees

### Fixed
- **Critical: FZF Deadlock**: Fixed pipe handling to prevent deadlock with large feeds
  - Properly closes stdin before waiting for output
  - Uses `take()` to move stdin ownership
  - Prevents buffer overflow deadlock
  - Handles write errors gracefully
- **Watch Mode Signal Handling**: Proper terminal cleanup on Ctrl+C
  - Restores cursor visibility
  - Clears screen state
  - Handles SIGINT gracefully
  - No more blank terminal after exit
- **Docker YAML Parser Brittleness**: Replaced simple parser with proper YAML library
  - Supports 4-space indentation
  - Supports tab indentation
  - Handles YAML anchors and aliases
  - Parses complex structures correctly
- **Hardcoded Shell Path**: Removed `/bin/bash` constant
  - Uses configured shell from config system
  - Detects user's shell from `$SHELL`
  - Works on NixOS, FreeBSD, macOS
  - Validates shell exists before use

### Changed
- **Tmux Session Creation**: Now uses configurable layout
  - Respects user preferences
  - Validates shell path
  - Uses config-specified commands
  - More flexible pane arrangement
- **Dependencies**: Added minimal necessary dependencies
  - `serde` 1.0.210 for serialization
  - `serde_yaml` 0.9.27 for YAML parsing
  - `log` 0.4 for logging
  - `env_logger` 0.10 for log output
  - `ctrlc` 3.4 for signal handling
  - `indexmap` 2.11.1 (pinned for stability)

### Performance
- **O(1) Config Lookups**: Config system caches values
- **Lazy YAML Parsing**: Only parses compose files when needed
- **Efficient Signal Handling**: Atomic flag for watch mode termination

### Documentation
- Added comprehensive config module documentation
- Added examples for all config options
- Documented signal handling behavior
- Explained layout customization options

## [0.1.0] - Initial Release

### Features
- Ticket-oriented worktree creation
- Tmux session integration
- Docker container access
- FZF search integration
- Shell integration for `cd` support
- Archive and prune commands
- Multiple output formats (text, JSON, shell)

[0.3.0]: https://github.com/user/gat/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/user/gat/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/user/gat/releases/tag/v0.1.0
