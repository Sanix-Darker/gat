//! Command-line parsing for `gat`.
//!
//! The parser is intentionally hand-written. Startup is faster, the binary has
//! no dependency graph, and the command surface is still small enough to keep
//! option handling explicit and easy to audit.

use crate::error::{GatError, Result};
use crate::output::OutputFormat;
use std::path::PathBuf;

/// Top-level command selected from CLI arguments.
#[derive(Debug)]
pub enum Command {
    /// Print global help.
    Help,
    /// Print version information.
    Version,
    /// Create or reuse a ticket worktree.
    New(NewArgs),
    /// Resolve an existing worktree path for shell switching.
    Go(GoArgs),
    /// Attach to (or create) the tmux session for an existing worktree.
    Switch(SwitchArgs),
    /// Set or show a worktree's description.
    Describe(DescribeArgs),
    /// List live gat tmux sessions.
    Sessions(SessionsArgs),
    /// Launch the interactive TUI dashboard.
    Ui(UiArgs),
    /// Inspect or edit gat configuration.
    Config(ConfigArgs),
    /// Merge a ticket branch into the default branch and clean up.
    Merge(MergeArgs),
    /// Print only the path to a worktree.
    Path(PathArgs),
    /// List registered worktrees.
    List(ListArgs),
    /// Repeatedly render the worktree list.
    Watch(WatchArgs),
    /// Search worktrees, optionally through `fzf`.
    Search(SearchArgs),
    /// Create or switch to a tmux session for a worktree.
    Tmux(TmuxArgs),
    /// Exec into or run a worktree-scoped Docker container.
    Docker(DockerArgs),
    /// Remove a worktree.
    Remove(RemoveArgs),
    /// Move a worktree to the archive area.
    Archive(ArchiveArgs),
    /// Prune stale or merged worktrees.
    Prune(PruneArgs),
    /// Emit shell integration code.
    ShellInit(ShellInitArgs),
    /// Inspect local setup and dependencies.
    Doctor(DoctorArgs),
}

/// Arguments for `gat new` and the shorthand `gat <ticket>`.
#[derive(Debug)]
pub struct NewArgs {
    /// Raw ticket or branch target provided by the user.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Optional branch override when the ticket is not the branch name.
    pub branch: Option<String>,
    /// Optional base ref for creating a new branch.
    pub base: Option<String>,
    /// Optional destination path override.
    pub path: Option<PathBuf>,
    /// Whether to create a detached investigation worktree.
    pub detach: bool,
    /// Optional description of what the worktree is for.
    pub description: Option<String>,
    /// Template to apply after creation. `None` uses the `default` template if
    /// one is configured; `Some("")` (via `--no-template`) disables templating.
    pub template: Option<String>,
    /// Show planned work without mutating Git or the filesystem.
    pub dry_run: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat go`.
#[derive(Debug)]
pub struct GoArgs {
    /// Ticket, branch, path basename, or shortcut.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Create the target worktree if it does not exist.
    pub create: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat switch`.
#[derive(Debug)]
pub struct SwitchArgs {
    /// Ticket or branch target naming an existing worktree.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Optional tmux session name override.
    pub session: Option<String>,
    /// Optional prompt draft path exported to tmux panes.
    pub prompt_file: Option<PathBuf>,
    /// Command run in the left AI pane when creating the session.
    pub codex_cmd: String,
    /// Editor command run in the right-top prompt pane when creating the session.
    pub editor_cmd: String,
    /// Optional layout preset name (classic, ai-focus, editor-focus, wide).
    pub layout: Option<String>,
    /// Whether to attach/switch to the tmux session.
    pub attach: bool,
    /// Print the planned action without making changes.
    pub dry_run: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat path`.
#[derive(Debug)]
pub struct PathArgs {
    /// Ticket, branch, path basename, or shortcut.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
}

/// Arguments for `gat describe`.
#[derive(Debug)]
pub struct DescribeArgs {
    /// Ticket or branch target naming an existing worktree.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// New description text. `None` prints the current description;
    /// `Some("")` clears it.
    pub description: Option<String>,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat sessions`.
#[derive(Debug)]
pub struct SessionsArgs {
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat ui`.
#[derive(Debug)]
pub struct UiArgs {
    /// Skip expensive dirty/merged/change-stat checks for faster startup.
    pub fast: bool,
}

/// A `gat config` sub-action.
#[derive(Debug)]
pub enum ConfigAction {
    /// Write a default config file (errors if it exists unless forced).
    Init { force: bool },
    /// Print the config file path.
    Path,
    /// List all keys and their effective values.
    List,
    /// Print a single key's value.
    Get { key: String },
    /// Set a key to a value and persist it.
    Set { key: String, value: String },
}

/// Arguments for `gat config`.
#[derive(Debug)]
pub struct ConfigArgs {
    /// The sub-action to perform.
    pub action: ConfigAction,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat merge`.
#[derive(Debug)]
pub struct MergeArgs {
    /// Ticket or branch target to merge.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Base branch to merge into; defaults to the repository default branch.
    pub into: Option<String>,
    /// Always create a merge commit (`--no-ff`).
    pub no_ff: bool,
    /// After a successful merge, remove the worktree.
    pub remove: bool,
    /// After removal, delete the merged branch.
    pub delete_branch: bool,
    /// After removal, kill the worktree's tmux session.
    pub kill_session: bool,
    /// Convenience: enable remove + delete_branch + kill_session.
    pub cleanup: bool,
    /// Skip confirmation prompts.
    pub yes: bool,
    /// Show the planned merge/cleanup without changing anything.
    pub dry_run: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat list`.
#[derive(Debug)]
pub struct ListArgs {
    /// Requested output format.
    pub format: OutputFormat,
    /// Skip expensive dirty/merged checks.
    pub fast: bool,
}

/// Arguments for `gat watch`.
#[derive(Debug)]
pub struct WatchArgs {
    /// Refresh interval in milliseconds.
    pub interval_ms: u64,
    /// Skip expensive dirty/merged checks.
    pub fast: bool,
    /// Render once and exit; useful for tests and scripting.
    pub once: bool,
}

/// Arguments for `gat search`.
#[derive(Debug)]
pub struct SearchArgs {
    /// Optional initial search query passed to `fzf`.
    pub query: Option<String>,
    /// Print the tab-separated feed instead of invoking `fzf`.
    pub print: bool,
    /// Print only the selected path.
    pub path: bool,
    /// Launch the selected item through `gat tmux`.
    pub tmux: bool,
    /// Disable `fzf` and print the feed.
    pub no_fzf: bool,
    /// Skip expensive dirty/merged checks.
    pub fast: bool,
    /// Requested output format for selected items.
    pub format: OutputFormat,
}

/// Arguments for `gat tmux`.
#[derive(Debug)]
pub struct TmuxArgs {
    /// Ticket or branch target.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Optional tmux session name override.
    pub session: Option<String>,
    /// Optional prompt draft path exported to tmux panes.
    pub prompt_file: Option<PathBuf>,
    /// Command run in the left AI pane.
    pub codex_cmd: String,
    /// Editor command run in the right-top prompt pane.
    pub editor_cmd: String,
    /// Optional layout preset name (classic, ai-focus, editor-focus, wide).
    pub layout: Option<String>,
    /// Whether to attach or switch to the tmux session.
    pub attach: bool,
    /// Print the tmux plan without making changes.
    pub dry_run: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat dx`.
#[derive(Debug)]
pub struct DockerArgs {
    /// Optional service override for both exec targeting and compose fallback.
    pub service: Option<String>,
    /// Print Docker/worktree diagnostics instead of running a command.
    pub doctor: bool,
    /// Command to run inside the container, default `bash`.
    pub command: Vec<String>,
}

/// Arguments for `gat archive`.
#[derive(Debug)]
pub struct ArchiveArgs {
    /// Ticket or branch target.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Optional archive root override.
    pub archive_dir: Option<PathBuf>,
    /// Permit archiving a dirty worktree.
    pub force: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// Show the planned archive move without changing anything.
    pub dry_run: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat rm` and `gat delete`.
#[derive(Debug)]
pub struct RemoveArgs {
    /// Ticket or branch target.
    pub target: String,
    /// Optional override for numeric ticket prefixes.
    pub prefix: Option<String>,
    /// Whether numeric ticket prefixing should be disabled.
    pub no_prefix: bool,
    /// Permit removal of a dirty worktree.
    pub force: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// Show planned removal without mutating state.
    pub dry_run: bool,
    /// Delete the local branch after removing the worktree.
    pub delete_branch: bool,
    /// Force-delete the local branch after removing the worktree.
    pub force_delete_branch: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat prune`.
#[derive(Debug)]
pub struct PruneArgs {
    /// Remove clean worktrees whose branches are merged into the default branch.
    pub merged: bool,
    /// Remove worktrees not accessed in N days.
    pub older_than_days: Option<u64>,
    /// Show planned pruning/removal without changing state.
    pub dry_run: bool,
    /// Skip confirmation prompts.
    pub yes: bool,
    /// Force removal even if worktrees are dirty.
    pub force: bool,
    /// Requested output format.
    pub format: OutputFormat,
}

/// Arguments for `gat shell-init`.
#[derive(Debug)]
pub struct ShellInitArgs {
    /// Shell dialect to generate.
    pub shell: Shell,
}

/// Arguments for `gat doctor`.
#[derive(Debug)]
pub struct DoctorArgs {
    /// Requested output format.
    pub format: OutputFormat,
}

/// Shell dialects supported by `gat shell-init`.
#[derive(Clone, Copy, Debug)]
pub enum Shell {
    /// POSIX-style bash function.
    Bash,
    /// POSIX-style zsh function.
    Zsh,
    /// Minimal fish function.
    Fish,
}

/// Command names reserved by the parser.
const COMMANDS: &[&str] = &[
    "help",
    "version",
    "new",
    "add",
    "go",
    "switch",
    "describe",
    "desc",
    "sessions",
    "ui",
    "dashboard",
    "config",
    "merge",
    "path",
    "list",
    "ls",
    "watch",
    "search",
    "find",
    "tmux",
    "session",
    "start",
    "dx",
    "docker",
    "rm",
    "remove",
    "delete",
    "archive",
    "prune",
    "shell-init",
    "doctor",
];

/// Parses process arguments into a [`Command`].
///
/// Unknown first positional arguments are treated as the shorthand
/// `gat new <ticket>` because that is the primary user workflow.
pub fn parse(args: Vec<String>) -> Result<Command> {
    if args.is_empty() {
        return Ok(Command::Help);
    }

    let first = args[0].as_str();
    match first {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "-V" | "--version" | "version" => Ok(Command::Version),
        "new" | "add" => parse_new(&args[1..]),
        "go" => parse_go(&args[1..]),
        "switch" => parse_switch(&args[1..]),
        "describe" | "desc" => parse_describe(&args[1..]),
        "sessions" => parse_sessions(&args[1..]),
        "ui" | "dashboard" => parse_ui(&args[1..]),
        "config" => parse_config(&args[1..]),
        "merge" => parse_merge(&args[1..]),
        "path" => parse_path(&args[1..]),
        "list" | "ls" => parse_list(&args[1..]),
        "watch" => parse_watch(&args[1..]),
        "search" | "find" => parse_search(&args[1..]),
        "tmux" | "session" | "start" => parse_tmux(&args[1..]),
        "dx" | "docker" => parse_docker(&args[1..]),
        "rm" | "remove" | "delete" => parse_remove(&args[1..]),
        "archive" => parse_archive(&args[1..]),
        "prune" => parse_prune(&args[1..]),
        "shell-init" => parse_shell_init(&args[1..]),
        "doctor" => parse_doctor(&args[1..]),
        value if value.starts_with('-') => Err(GatError::Usage(format!(
            "unknown option {value}; run `gat --help`"
        ))),
        value if COMMANDS.contains(&value) => unreachable!(),
        _ => parse_new(&args),
    }
}

/// Parses `gat new` and shorthand ticket creation options.
fn parse_new(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut branch = None;
    let mut base = None;
    let mut path = None;
    let mut detach = false;
    let mut description = None;
    let mut template = None;
    let mut dry_run = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--branch" => {
                i += 1;
                branch = Some(require_value(args, i, "--branch")?.to_string());
            }
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--base" | "--from" | "--from-ref" => {
                i += 1;
                base = Some(require_value(args, i, args[i - 1].as_str())?.to_string());
            }
            "--path" => {
                i += 1;
                path = Some(PathBuf::from(require_value(args, i, "--path")?));
            }
            "--detach" => detach = true,
            "--description" | "-d" | "--desc" => {
                i += 1;
                description = Some(require_value(args, i, "--description")?.to_string());
            }
            "--template" | "-t" => {
                i += 1;
                template = Some(require_value(args, i, "--template")?.to_string());
            }
            "--no-template" => template = Some(String::new()),
            "--dry-run" | "-n" => dry_run = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            "--shell" => format = OutputFormat::Shell,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--description=") => {
                description = Some(value.trim_start_matches("--description=").to_string());
            }
            value if value.starts_with("--desc=") => {
                description = Some(value.trim_start_matches("--desc=").to_string());
            }
            value if value.starts_with("--template=") => {
                template = Some(value.trim_start_matches("--template=").to_string());
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown new option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}; create one worktree at a time"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::New(NewArgs {
        target: target.ok_or_else(|| GatError::Usage("missing ticket or branch".into()))?,
        prefix,
        no_prefix,
        branch,
        base,
        path,
        detach,
        description,
        template,
        dry_run,
        format,
    }))
}

/// Parses `gat go` options.
fn parse_go(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut create = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--create" | "-c" => create = true,
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            "--shell" => format = OutputFormat::Shell,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(GatError::Usage(format!("unknown go option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Go(GoArgs {
        target: target.ok_or_else(|| GatError::Usage("missing worktree target".into()))?,
        prefix,
        no_prefix,
        create,
        format,
    }))
}

/// Parses `gat switch` options.
///
/// `switch` mirrors `tmux`'s option surface so the two commands behave
/// consistently, but it never creates a worktree: the target must already
/// exist.
fn parse_switch(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut session = None;
    let mut prompt_file = None;
    let mut codex_cmd = "codex".to_string();
    let mut editor_cmd = "nvim".to_string();
    let mut layout = None;
    let mut attach = true;
    let mut dry_run = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--session" => {
                i += 1;
                session = Some(require_value(args, i, "--session")?.to_string());
            }
            "--prompt-file" => {
                i += 1;
                prompt_file = Some(PathBuf::from(require_value(args, i, "--prompt-file")?));
            }
            "--codex-cmd" => {
                i += 1;
                codex_cmd = require_value(args, i, "--codex-cmd")?.to_string();
            }
            "--editor-cmd" => {
                i += 1;
                editor_cmd = require_value(args, i, "--editor-cmd")?.to_string();
            }
            "--layout" => {
                i += 1;
                layout = Some(require_value(args, i, "--layout")?.to_string());
            }
            "--no-attach" => attach = false,
            "--dry-run" | "-n" => dry_run = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--session=") => {
                session = Some(value.trim_start_matches("--session=").to_string());
            }
            value if value.starts_with("--prompt-file=") => {
                prompt_file = Some(PathBuf::from(value.trim_start_matches("--prompt-file=")));
            }
            value if value.starts_with("--codex-cmd=") => {
                codex_cmd = value.trim_start_matches("--codex-cmd=").to_string();
            }
            value if value.starts_with("--editor-cmd=") => {
                editor_cmd = value.trim_start_matches("--editor-cmd=").to_string();
            }
            value if value.starts_with("--layout=") => {
                layout = Some(value.trim_start_matches("--layout=").to_string());
            }
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown switch option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Switch(SwitchArgs {
        target: target.ok_or_else(|| GatError::Usage("switch requires a target".into()))?,
        prefix,
        no_prefix,
        session,
        prompt_file,
        codex_cmd,
        editor_cmd,
        layout,
        attach,
        dry_run,
        format,
    }))
}

/// Parses `gat describe` options.
///
/// Usage: `gat describe <ticket> [text...]`. With no text, the current
/// description is printed. Multiple trailing words are joined into one
/// description. `--clear` removes the description.
fn parse_describe(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut clear = false;
    let mut format = OutputFormat::Text;
    let mut words: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--clear" => clear = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(GatError::Usage(format!("unknown describe option {value}")));
            }
            value => {
                if target.is_none() {
                    target = Some(value.to_string());
                } else {
                    words.push(value.to_string());
                }
            }
        }
        i += 1;
    }

    let description = if clear {
        Some(String::new())
    } else if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    };

    Ok(Command::Describe(DescribeArgs {
        target: target.ok_or_else(|| GatError::Usage("describe requires a target".into()))?,
        prefix,
        no_prefix,
        description,
        format,
    }))
}

/// Parses `gat sessions` options.
fn parse_sessions(args: &[String]) -> Result<Command> {
    let mut format = OutputFormat::Text;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value => return Err(GatError::Usage(format!("unknown sessions option {value}"))),
        }
        i += 1;
    }
    Ok(Command::Sessions(SessionsArgs { format }))
}

/// Parses `gat ui` options.
fn parse_ui(args: &[String]) -> Result<Command> {
    let mut fast = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--fast" => fast = true,
            value => return Err(GatError::Usage(format!("unknown ui option {value}"))),
        }
        i += 1;
    }
    Ok(Command::Ui(UiArgs { fast }))
}

/// Parses `gat config` sub-commands.
///
/// Usage:
///   `gat config init [--force]`
///   `gat config path`
///   `gat config list`
///   `gat config get <key>`
///   `gat config set <key> <value>`
fn parse_config(args: &[String]) -> Result<Command> {
    let mut format = OutputFormat::Text;
    let mut positional: Vec<String> = Vec::new();
    let mut force = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--force" | "-f" => force = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown config option {value}")));
            }
            value => positional.push(value.to_string()),
        }
        i += 1;
    }

    let sub = positional.first().map(String::as_str).ok_or_else(|| {
        GatError::Usage("config requires a subcommand: init|path|list|get|set".into())
    })?;

    let action = match sub {
        "init" => ConfigAction::Init { force },
        "path" => ConfigAction::Path,
        "list" => ConfigAction::List,
        "get" => {
            let key = positional
                .get(1)
                .cloned()
                .ok_or_else(|| GatError::Usage("config get requires a key".into()))?;
            ConfigAction::Get { key }
        }
        "set" => {
            let key = positional
                .get(1)
                .cloned()
                .ok_or_else(|| GatError::Usage("config set requires a key and value".into()))?;
            // Join remaining words so values with spaces work unquoted.
            let value = positional
                .get(2..)
                .filter(|rest| !rest.is_empty())
                .map(|rest| rest.join(" "))
                .ok_or_else(|| GatError::Usage("config set requires a value".into()))?;
            ConfigAction::Set { key, value }
        }
        other => {
            return Err(GatError::Usage(format!(
                "unknown config subcommand '{other}'; expected init|path|list|get|set"
            )))
        }
    };

    Ok(Command::Config(ConfigArgs { action, format }))
}

/// Parses `gat merge` options.
fn parse_merge(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut into = None;
    let mut no_ff = false;
    let mut remove = false;
    let mut delete_branch = false;
    let mut kill_session = false;
    let mut cleanup = false;
    let mut yes = false;
    let mut dry_run = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--into" => {
                i += 1;
                into = Some(require_value(args, i, "--into")?.to_string());
            }
            "--no-ff" => no_ff = true,
            "--remove" | "--rm" => remove = true,
            "--delete-branch" => {
                remove = true;
                delete_branch = true;
            }
            "--kill-session" => kill_session = true,
            "--cleanup" => cleanup = true,
            "--yes" | "-y" => yes = true,
            "--dry-run" | "-n" => dry_run = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--into=") => {
                into = Some(value.trim_start_matches("--into=").to_string());
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown merge option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Merge(MergeArgs {
        target: target.ok_or_else(|| GatError::Usage("merge requires a target".into()))?,
        prefix,
        no_prefix,
        into,
        no_ff,
        remove,
        delete_branch,
        kill_session,
        cleanup,
        yes,
        dry_run,
        format,
    }))
}

/// Parses `gat path` options.
fn parse_path(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(GatError::Usage(format!("unknown path option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Path(PathArgs {
        target: target.ok_or_else(|| GatError::Usage("path requires a target".into()))?,
        prefix,
        no_prefix,
    }))
}

/// Parses `gat list` options.
fn parse_list(args: &[String]) -> Result<Command> {
    let mut format = OutputFormat::Text;
    let mut fast = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--fast" => fast = true,
            "--watch" => {
                return Ok(Command::Watch(WatchArgs {
                    interval_ms: 1000,
                    fast: true,
                    once: false,
                }))
            }
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value => return Err(GatError::Usage(format!("unknown list option {value}"))),
        }
        i += 1;
    }
    Ok(Command::List(ListArgs { format, fast }))
}

/// Parses `gat watch` options.
fn parse_watch(args: &[String]) -> Result<Command> {
    let mut interval_ms = 1000;
    let mut fast = true;
    let mut once = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--interval" => {
                i += 1;
                interval_ms = parse_u64(require_value(args, i, "--interval")?, "--interval")?;
            }
            "--once" => once = true,
            "--full" => fast = false,
            "--fast" => fast = true,
            value if value.starts_with("--interval=") => {
                interval_ms = parse_u64(value.trim_start_matches("--interval="), "--interval")?;
            }
            value => return Err(GatError::Usage(format!("unknown watch option {value}"))),
        }
        i += 1;
    }

    Ok(Command::Watch(WatchArgs {
        interval_ms,
        fast,
        once,
    }))
}

/// Parses `gat search` options.
fn parse_search(args: &[String]) -> Result<Command> {
    let mut query = None;
    let mut print = false;
    let mut path = false;
    let mut tmux = false;
    let mut no_fzf = false;
    let mut fast = true;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--print" => print = true,
            "--path" => path = true,
            "--tmux" => tmux = true,
            "--no-fzf" => no_fzf = true,
            "--full" => fast = false,
            "--fast" => fast = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            "--shell" => format = OutputFormat::Shell,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown search option {value}")));
            }
            value => {
                if query.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                query = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Search(SearchArgs {
        query,
        print,
        path,
        tmux,
        no_fzf,
        fast,
        format,
    }))
}

/// Parses `gat tmux` options.
fn parse_tmux(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut session = None;
    let mut prompt_file = None;
    let mut codex_cmd = "codex".to_string();
    let mut editor_cmd = "nvim".to_string();
    let mut layout = None;
    let mut attach = true;
    let mut dry_run = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--session" => {
                i += 1;
                session = Some(require_value(args, i, "--session")?.to_string());
            }
            "--prompt-file" => {
                i += 1;
                prompt_file = Some(PathBuf::from(require_value(args, i, "--prompt-file")?));
            }
            "--codex-cmd" => {
                i += 1;
                codex_cmd = require_value(args, i, "--codex-cmd")?.to_string();
            }
            "--editor-cmd" => {
                i += 1;
                editor_cmd = require_value(args, i, "--editor-cmd")?.to_string();
            }
            "--layout" => {
                i += 1;
                layout = Some(require_value(args, i, "--layout")?.to_string());
            }
            "--no-attach" => attach = false,
            "--dry-run" | "-n" => dry_run = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--session=") => {
                session = Some(value.trim_start_matches("--session=").to_string());
            }
            value if value.starts_with("--prompt-file=") => {
                prompt_file = Some(PathBuf::from(value.trim_start_matches("--prompt-file=")));
            }
            value if value.starts_with("--codex-cmd=") => {
                codex_cmd = value.trim_start_matches("--codex-cmd=").to_string();
            }
            value if value.starts_with("--editor-cmd=") => {
                editor_cmd = value.trim_start_matches("--editor-cmd=").to_string();
            }
            value if value.starts_with("--layout=") => {
                layout = Some(value.trim_start_matches("--layout=").to_string());
            }
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown tmux option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Tmux(TmuxArgs {
        target: target.ok_or_else(|| GatError::Usage("tmux requires a target".into()))?,
        prefix,
        no_prefix,
        session,
        prompt_file,
        codex_cmd,
        editor_cmd,
        layout,
        attach,
        dry_run,
        format,
    }))
}

/// Parses `gat dx` options.
fn parse_docker(args: &[String]) -> Result<Command> {
    let mut service = None;
    let mut doctor = false;
    let mut command = Vec::new();
    let mut passthrough = false;

    let mut i = 0;
    while i < args.len() {
        if passthrough {
            command.push(args[i].clone());
            i += 1;
            continue;
        }

        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--service" => {
                i += 1;
                service = Some(require_value(args, i, "--service")?.to_string());
            }
            "--doctor" => doctor = true,
            "--" => passthrough = true,
            value if value.starts_with("--service=") => {
                service = Some(value.trim_start_matches("--service=").to_string());
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown dx option {value}")));
            }
            value => {
                command.push(value.to_string());
                passthrough = true;
            }
        }
        i += 1;
    }

    if doctor && !command.is_empty() {
        return Err(GatError::Usage(
            "dx --doctor does not accept a command".into(),
        ));
    }

    Ok(Command::Docker(DockerArgs {
        service,
        doctor,
        command,
    }))
}

/// Parses `gat rm`, `gat remove`, and `gat delete` options.
fn parse_remove(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut force = false;
    let mut yes = false;
    let mut dry_run = false;
    let mut delete_branch = false;
    let mut force_delete_branch = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--force" | "-f" => force = true,
            "--yes" | "-y" => yes = true,
            "--dry-run" | "-n" => dry_run = true,
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--delete-branch" => delete_branch = true,
            "--force-delete-branch" => {
                delete_branch = true;
                force_delete_branch = true;
            }
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown remove option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Remove(RemoveArgs {
        target: target.ok_or_else(|| GatError::Usage("missing worktree target".into()))?,
        prefix,
        no_prefix,
        force,
        yes,
        dry_run,
        delete_branch,
        force_delete_branch,
        format,
    }))
}

/// Parses `gat archive` options.
fn parse_archive(args: &[String]) -> Result<Command> {
    let mut target = None;
    let mut prefix = None;
    let mut no_prefix = false;
    let mut archive_dir = None;
    let mut force = false;
    let mut yes = false;
    let mut dry_run = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--prefix" => {
                i += 1;
                prefix = Some(require_value(args, i, "--prefix")?.to_string());
            }
            "--no-prefix" => no_prefix = true,
            "--archive-dir" => {
                i += 1;
                archive_dir = Some(PathBuf::from(require_value(args, i, "--archive-dir")?));
            }
            "--force" | "-f" => force = true,
            "--yes" | "-y" => yes = true,
            "--dry-run" | "-n" => dry_run = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--prefix=") => {
                prefix = Some(value.trim_start_matches("--prefix=").to_string());
            }
            value if value.starts_with("--archive-dir=") => {
                archive_dir = Some(PathBuf::from(value.trim_start_matches("--archive-dir=")));
            }
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with('-') => {
                return Err(GatError::Usage(format!("unknown archive option {value}")));
            }
            value => {
                if target.is_some() {
                    return Err(GatError::Usage(format!(
                        "unexpected extra argument {value}"
                    )));
                }
                target = Some(value.to_string());
            }
        }
        i += 1;
    }

    Ok(Command::Archive(ArchiveArgs {
        target: target.ok_or_else(|| GatError::Usage("archive requires a target".into()))?,
        prefix,
        no_prefix,
        archive_dir,
        force,
        yes,
        dry_run,
        format,
    }))
}

/// Parses `gat prune` options.
fn parse_prune(args: &[String]) -> Result<Command> {
    let mut merged = false;
    let mut older_than_days = None;
    let mut dry_run = false;
    let mut yes = false;
    let mut force = false;
    let mut format = OutputFormat::Text;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--merged" => merged = true,
            "--older-than" => {
                i += 1;
                older_than_days = Some(parse_u64(
                    require_value(args, i, "--older-than")?,
                    "--older-than",
                )?);
            }
            "--dry-run" | "-n" => dry_run = true,
            "--yes" | "-y" => yes = true,
            "--force" | "-f" => force = true,
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value if value.starts_with("--older-than=") => {
                older_than_days = Some(parse_u64(
                    value.trim_start_matches("--older-than="),
                    "--older-than",
                )?);
            }
            value => return Err(GatError::Usage(format!("unknown prune option {value}"))),
        }
        i += 1;
    }

    Ok(Command::Prune(PruneArgs {
        merged,
        older_than_days,
        dry_run,
        yes,
        force,
        format,
    }))
}

/// Parses `gat shell-init` options.
fn parse_shell_init(args: &[String]) -> Result<Command> {
    let mut shell = Shell::Bash;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--shell" => {
                i += 1;
                shell = parse_shell(require_value(args, i, "--shell")?)?;
            }
            value if value.starts_with("--shell=") => {
                shell = parse_shell(value.trim_start_matches("--shell="))?;
            }
            value => {
                return Err(GatError::Usage(format!(
                    "unknown shell-init option {value}"
                )))
            }
        }
        i += 1;
    }
    Ok(Command::ShellInit(ShellInitArgs { shell }))
}

/// Parses `gat doctor` options.
fn parse_doctor(args: &[String]) -> Result<Command> {
    let mut format = OutputFormat::Text;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--format" => {
                i += 1;
                format = parse_format(require_value(args, i, "--format")?)?;
            }
            "--json" => format = OutputFormat::Json,
            value if value.starts_with("--format=") => {
                format = parse_format(value.trim_start_matches("--format="))?;
            }
            value => return Err(GatError::Usage(format!("unknown doctor option {value}"))),
        }
        i += 1;
    }
    Ok(Command::Doctor(DoctorArgs { format }))
}

/// Returns a required value after an option flag.
fn require_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| GatError::Usage(format!("{flag} requires a value")))
}

/// Parses `--format`.
fn parse_format(value: &str) -> Result<OutputFormat> {
    OutputFormat::parse(value).ok_or_else(|| {
        GatError::Usage(format!(
            "invalid format {value}; expected one of: text, json, shell"
        ))
    })
}

/// Parses `--shell`.
fn parse_shell(value: &str) -> Result<Shell> {
    match value {
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "fish" => Ok(Shell::Fish),
        _ => Err(GatError::Usage(format!(
            "invalid shell {value}; expected bash, zsh, or fish"
        ))),
    }
}

/// Parses positive integer option values.
fn parse_u64(value: &str, flag: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| GatError::Usage(format!("{flag} requires a positive integer")))?;
    if parsed == 0 {
        return Err(GatError::Usage(format!(
            "{flag} requires a positive integer"
        )));
    }
    Ok(parsed)
}

/// Returns global help text.
pub fn help() -> &'static str {
    r#"gat - ticket-oriented Git worktree helper

Usage:
  gat <ticket> [options]              Create or reuse a ticket worktree
  gat new <ticket> [options]          Create or reuse a ticket worktree
  gat go <ticket|-|^|@> [options]     Print/switch to an existing worktree
  gat switch <ticket> [options]       Attach to (or open) the worktree's tmux session
  gat describe <ticket> [text]        Set/show a worktree description
  gat sessions [--format json]        List live gat tmux sessions
  gat ui [--fast]                     Interactive TUI dashboard (requires tui feature)
  gat config <init|path|list|get|set> Inspect or edit configuration
  gat path <ticket>                   Print a worktree path only
  gat list [--format text|json]       List worktrees
  gat watch [--interval 1000]         Watch worktrees
  gat search [query] [options]        Search worktrees with fzf or feed output
  gat tmux <ticket> [options]         Open Codex/nvim/shell tmux panes
  gat dx [options] [command ...]      Enter or run a worktree Docker container
  gat rm <ticket> [options]           Remove a worktree
  gat delete <ticket> [options]       Alias for rm
  gat archive <ticket> [options]      Move worktree to archive directory
  gat merge <ticket> [options]        Merge a ticket branch into the default branch
  gat prune [--merged] [options]      Prune stale or merged worktrees
  gat shell-init [--shell bash|zsh]   Print shell integration
  gat doctor                          Inspect local setup
  gat --version                       Print version

New options:
  --prefix <prefix>                   Prefix numeric tickets, default TICKET
  --no-prefix                         Keep numeric tickets unprefixed
  --base, --from <ref>                Create new branch from ref
  --branch <branch>                   Use branch name instead of ticket
  --path <path>                       Override computed worktree path
  --detach                            Create detached investigation worktree
  -d, --description <text>            Describe what the worktree is for
  -t, --template <name>               Apply a setup template after creation
  --no-template                       Skip templating even if a default exists
  -n, --dry-run                       Show planned action only
  --format text|json|shell            Output format

Target options for go/path/rm:
  --prefix <prefix>                   Prefix numeric ticket target
  --no-prefix                         Keep numeric target unprefixed

Tmux options:
  --session <name>                    Override tmux session name
  --prompt-file <path>                Prompt draft path, default ~/.config/gat
  --codex-cmd <cmd>                   Command for left pane, default codex
  --editor-cmd <cmd>                  Editor for prompt pane, default nvim
  --layout <preset>                   Layout preset: classic, ai-focus, editor-focus, wide
  --no-attach                         Create/reuse without attaching
  -n, --dry-run                       Print tmux plan only
  --format text|json|shell            Output format

Switch options:
  (existing worktree only; never creates a worktree)
  --session <name>                    Override tmux session name
  --prompt-file <path>                Prompt draft path used when creating panes
  --codex-cmd <cmd>                   Command for left pane, default codex
  --editor-cmd <cmd>                  Editor for prompt pane, default nvim
  --layout <preset>                   Layout preset: classic, ai-focus, editor-focus, wide
  --no-attach                         Ensure session without attaching
  -n, --dry-run                       Print switch plan only
  --format text|json|shell            Output format

Docker options:
  --service <name>                    Compose service to exec/run
  --doctor                            Print docker/worktree diagnostics

Search options:
  --print                             Print tab-separated worktree feed
  --path                              Print selected path only
  --tmux                              Open selected worktree tmux session
  --no-fzf                            Do not invoke fzf
  --full                              Include dirty/merged checks

Archive options:
  --archive-dir <path>                Archive root, default ../<repo>-archive
  -f, --force                         Archive dirty worktree
  -y, --yes                           Skip confirmation prompt

Remove options:
  -f, --force                         Remove dirty worktree
  -y, --yes                           Skip confirmation prompt
  --delete-branch                     Delete branch after removing worktree
  --force-delete-branch               Force delete branch after removal
  -n, --dry-run                       Show planned removal only

Prune options:
  --merged                            Remove merged, clean worktrees
  --older-than <days>                 Remove worktrees unused for N days
  -f, --force                         Include dirty worktrees when pruning by age
  -y, --yes                           Skip confirmation prompts
  -n, --dry-run                       Show planned pruning only

Merge options:
  --into <branch>                     Merge into this branch (default: repo default)
  --no-ff                             Always create a merge commit
  --remove                            Remove the worktree after a successful merge
  --delete-branch                     Remove worktree and delete the merged branch
  --kill-session                      Kill the worktree's tmux session after merge
  --cleanup                           Shorthand for --remove --delete-branch --kill-session
  -y, --yes                           Skip confirmation prompts
  -n, --dry-run                       Show planned merge/cleanup only
"#
}
