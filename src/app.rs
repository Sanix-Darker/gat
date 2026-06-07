//! Application layer for `gat` commands.
//!
//! This module coordinates parsed CLI arguments with Git, filesystem, shell,
//! `fzf`, and `tmux` operations. It deliberately keeps business rules here so
//! lower-level modules stay thin and predictable.

use crate::cli::{
    self, ArchiveArgs, Command, ConfigAction, ConfigArgs, DescribeArgs, DockerArgs, DoctorArgs,
    GoArgs, ListArgs, MergeArgs, NewArgs, PathArgs, PruneArgs, RemoveArgs, SearchArgs,
    SessionsArgs, Shell, ShellInitArgs, SwitchArgs, TmuxArgs, UiArgs, WatchArgs,
};
use crate::config;
use crate::docker;
use crate::error::{GatError, Result};
use crate::git::{self, Repo, Worktree};
use crate::metadata::MetadataStore;
use crate::output::{json_escape, path_string, shell_escape, OutputFormat};
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::thread;
use std::time::Duration;

/// Plan for creating or reusing a ticket worktree.
///
/// The plan is built before mutation so dry-runs, JSON output, tmux launching,
/// and actual creation all share one decision path.
#[derive(Debug)]
struct NewPlan {
    /// Normalized ticket name, e.g. `TICKET-12345`.
    ticket: String,
    /// Branch that should back the worktree.
    branch: String,
    /// Base ref for new branch creation.
    base: String,
    /// Destination worktree path.
    path: PathBuf,
    /// Whether this command will create, reuse, or only report.
    action: NewAction,
    /// Whether the worktree should be detached.
    detach: bool,
}

/// Planned worktree creation action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NewAction {
    /// Create a new worktree.
    Create,
    /// Reuse a registered worktree.
    Existing,
    /// Report the plan without mutating state.
    DryRun,
}

/// Worktree plus status fields computed by `gat list` and `gat search`.
#[derive(Debug)]
struct ListedWorktree {
    /// Parsed Git worktree metadata.
    worktree: Worktree,
    /// Whether the worktree has uncommitted changes.
    dirty: bool,
    /// Whether the branch is merged into the default branch.
    merged: bool,
    /// Number of changed paths in the working tree (staged + unstaged + untracked).
    changed_files: usize,
    /// Inserted lines versus `HEAD` for tracked changes.
    insertions: usize,
    /// Deleted lines versus `HEAD` for tracked changes.
    deletions: usize,
    /// Days since the worktree was last accessed, if known.
    idle_days: Option<u64>,
    /// Optional free-form description of the worktree.
    description: Option<String>,
}

/// Executes a parsed command and returns text for stdout.
pub fn run(command: Command) -> Result<String> {
    match command {
        Command::Help => Ok(cli::help().to_string()),
        Command::Version => Ok(format!("gat {}\n", env!("CARGO_PKG_VERSION"))),
        Command::New(args) => new_worktree(args),
        Command::Go(args) => go_worktree(args),
        Command::Switch(args) => switch_session(args),
        Command::Describe(args) => describe_worktree(args),
        Command::Sessions(args) => list_sessions(args),
        Command::Ui(args) => run_ui(args),
        Command::Config(args) => config_command(args),
        Command::Merge(args) => merge_command(args),
        Command::Path(args) => path_worktree(args),
        Command::List(args) => list_worktrees(args),
        Command::Watch(args) => watch_worktrees(args),
        Command::Search(args) => search_worktrees(args),
        Command::Tmux(args) => tmux_session(args),
        Command::Docker(args) => docker_worktree(args),
        Command::Remove(args) => remove_worktree(args),
        Command::Archive(args) => archive_worktree(args),
        Command::Prune(args) => prune_worktrees(args),
        Command::ShellInit(args) => shell_init(args),
        Command::Doctor(args) => doctor(args),
    }
}

/// Records worktree creation in the metadata store, logging on failure.
///
/// Metadata tracking is best-effort: a failure to persist usage stats must
/// never abort the primary worktree operation.
fn track_worktree_creation(repo: &Repo, path: &Path, branch: &str) {
    let path = path_string(path);
    let branch = branch.to_string();
    if let Err(e) = MetadataStore::update(&repo.current_root, |m| m.track_creation(&path, &branch))
    {
        log::warn!("Failed to record worktree creation metadata: {e}");
    }
}

/// Records worktree access in the metadata store, logging on failure.
fn track_worktree_access(repo: &Repo, path: &Path) {
    let path = path_string(path);
    if let Err(e) = MetadataStore::update(&repo.current_root, |m| m.track_access(&path)) {
        log::warn!("Failed to record worktree access metadata: {e}");
    }
}

/// Removes worktree metadata, logging on failure.
fn untrack_worktree(repo: &Repo, path: &Path) {
    let path = path_string(path);
    if let Err(e) = MetadataStore::update(&repo.current_root, |m| m.remove(&path)) {
        log::warn!("Failed to remove worktree metadata: {e}");
    }
}

/// Applies a setup template to a newly created worktree.
///
/// Resolution: an explicit non-empty `requested` name must exist or it is an
/// error; an empty string (`--no-template`) skips templating; `None` applies a
/// `default` template only if one is configured. Copy/symlink sources are
/// resolved relative to the primary worktree, destinations relative to the new
/// worktree. Individual copy/symlink failures warn but do not abort; a failing
/// `run` command aborts so setup problems are visible.
fn apply_template(repo: &Repo, worktree_path: &Path, requested: Option<&str>) -> Result<()> {
    // `--no-template` disables templating entirely.
    if requested == Some("") {
        return Ok(());
    }

    let config = config::GatConfig::load(Some(repo))?;
    let name = match requested {
        Some(name) => name,
        None => {
            if config.templates.contains_key("default") {
                "default"
            } else {
                return Ok(());
            }
        }
    };

    let template = match config.templates.get(name) {
        Some(t) => t,
        None => {
            // An explicitly requested but missing template is a usage error.
            return Err(GatError::NotFound(format!(
                "template '{name}' is not configured; add a [template.{name}] section to your config"
            )));
        }
    };

    progress(&format!("applying template '{name}'"));
    let source_root = &repo.primary_root;

    // Copy files.
    for rel in &template.copy {
        let src = source_root.join(rel);
        let dst = worktree_path.join(rel);
        if !src.exists() {
            log::warn!(
                "template copy source missing, skipping: {}",
                path_string(&src)
            );
            continue;
        }
        if let Some(parent) = dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "template copy: failed to create {}: {e}",
                    path_string(parent)
                );
                continue;
            }
        }
        match std::fs::copy(&src, &dst) {
            Ok(_) => progress(&format!("copied {rel}")),
            Err(e) => log::warn!("template copy failed for {rel}: {e}"),
        }
    }

    // Symlink directories/files.
    for rel in &template.symlink {
        let src = source_root.join(rel);
        let dst = worktree_path.join(rel);
        if !src.exists() {
            log::warn!(
                "template symlink source missing, skipping: {}",
                path_string(&src)
            );
            continue;
        }
        if dst.exists() {
            log::warn!(
                "template symlink target exists, skipping: {}",
                path_string(&dst)
            );
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match symlink_path(&src, &dst) {
            Ok(_) => progress(&format!("symlinked {rel}")),
            Err(e) => log::warn!("template symlink failed for {rel}: {e}"),
        }
    }

    // Run setup commands in the new worktree.
    for command in &template.run {
        progress(&format!("running: {command}"));
        let status = ProcessCommand::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(worktree_path)
            .status()
            .map_err(|e| {
                GatError::Io(format!("failed to run template command '{command}': {e}"))
            })?;
        if !status.success() {
            return Err(GatError::Io(format!(
                "template command failed ({}): {command}",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            )));
        }
    }

    Ok(())
}

/// Creates a symbolic link at `dst` pointing to `src` (platform-specific).
#[cfg(unix)]
fn symlink_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

/// Creates a symbolic link at `dst` pointing to `src` (platform-specific).
#[cfg(not(unix))]
fn symlink_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    // On non-unix, fall back to a directory symlink which covers the common
    // dependency-directory use case.
    std::os::windows::fs::symlink_dir(src, dst)
}

/// Creates or reuses a ticket worktree.
///
/// Existing registered worktrees win over creation so `gat 12345` is idempotent.
fn new_worktree(args: NewArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let plan = build_new_plan(&repo, &worktrees, &args)?;

    if plan.action == NewAction::DryRun {
        return format_new_result(&plan, args.format);
    }

    if plan.action == NewAction::Create {
        ensure_parent_exists(&plan.path)?;
        progress(&format!(
            "creating worktree for {} on branch {} at {}",
            plan.ticket,
            plan.branch,
            path_string(&plan.path)
        ));
        git::add_worktree(&repo, &plan.path, &plan.branch, &plan.base, plan.detach)?;
        progress(&format!(
            "git worktree created at {}",
            path_string(&plan.path)
        ));
        track_worktree_creation(&repo, &plan.path, &plan.branch);

        // Apply a setup template to the freshly created worktree.
        apply_template(&repo, &plan.path, args.template.as_deref())?;
    }

    // Apply an explicit description for both new and reused worktrees.
    if let Some(description) = args.description.as_deref() {
        let path = path_string(&plan.path);
        let branch = plan.branch.clone();
        if let Err(e) = MetadataStore::update(&repo.current_root, |m| {
            m.set_description(&path, &branch, description);
        }) {
            log::warn!("Failed to save worktree description: {e}");
        }
    }

    format_new_result(&plan, args.format)
}

/// Resolves a worktree path for shell switching.
///
/// With `--create`, this becomes a create-or-switch command.
fn go_worktree(args: GoArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    if let Some(path) = resolve_shortcut_path(&repo, &worktrees, &target)? {
        track_worktree_access(&repo, &path);
        return format_path_result("switch", &target, &path, args.format);
    }

    if let Some(wt) = git::find_worktree(&worktrees, &target) {
        track_worktree_access(&repo, &wt.path);
        return format_path_result("switch", &target, &wt.path, args.format);
    }

    if args.create {
        return new_worktree(NewArgs {
            target,
            prefix: None,
            no_prefix: true,
            branch: None,
            base: None,
            path: None,
            detach: false,
            description: None,
            template: None,
            dry_run: false,
            format: args.format,
        });
    }

    Err(GatError::NotFound(format!(
        "no worktree found for {}",
        target
    )))
}

/// Prints only the resolved worktree path.
fn path_worktree(args: PathArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    if let Some(path) = resolve_shortcut_path(&repo, &worktrees, &target)? {
        return Ok(format!("{}\n", path_string(&path)));
    }
    let wt = git::find_worktree(&worktrees, &target)
        .ok_or_else(|| GatError::NotFound(format!("no worktree found for {}", target)))?;
    Ok(format!("{}\n", path_string(&wt.path)))
}

/// Lists all registered worktrees in text, shell, or JSON format.
fn list_worktrees(args: ListArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let listed = collect_listed_worktrees(&repo, args.fast)?;

    match args.format {
        OutputFormat::Text | OutputFormat::Shell => Ok(format_list_text(&repo, &listed)),
        OutputFormat::Json => Ok(format_list_json(&listed)),
    }
}

/// Re-renders the worktree list until interrupted.
///
/// `--once` exists for tests and one-shot script use.
fn watch_worktrees(args: WatchArgs) -> Result<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Set up Ctrl+C handler to restore terminal state
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .unwrap_or_else(|e| {
        log::warn!("Failed to set Ctrl+C handler: {e}");
    });

    loop {
        if !running.load(Ordering::SeqCst) {
            // Restore cursor and clear screen on exit
            print!("\x1b[?25h"); // Show cursor
            io::stdout().flush()?;
            return Ok(String::new());
        }

        let repo = git::discover_repo()?;
        let listed = collect_listed_worktrees(&repo, args.fast)?;

        print!("\x1b[2J\x1b[H\x1b[?25l"); // Clear screen, home cursor, hide cursor
        println!("{}", format_list_text(&repo, &listed).trim_end());
        io::stdout().flush()?;

        if args.once {
            print!("\x1b[?25h"); // Show cursor
            io::stdout().flush()?;
            return Ok(String::new());
        }

        thread::sleep(Duration::from_millis(args.interval_ms.max(100)));
    }
}

/// Searches worktrees through a stable feed or interactive `fzf`.
///
/// In shell format, a selected item emits `GAT_PATH=...` so shell integration
/// can immediately `cd` into the chosen worktree.
fn search_worktrees(args: SearchArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let listed = collect_listed_worktrees(&repo, args.fast)?;
    let feed = format_search_feed(&listed);

    if args.print || args.no_fzf {
        return Ok(feed);
    }

    if !command_exists("fzf") {
        return Err(GatError::NotFound(
            "fzf not found; use `gat search --print` or install fzf".into(),
        ));
    }

    let selected = run_fzf(&feed, args.query.as_deref())?;
    if selected.trim().is_empty() {
        return Ok(String::new());
    }
    let Some((branch, path)) = parse_search_selection(&selected) else {
        return Err(GatError::Io("could not parse fzf selection".into()));
    };

    if args.tmux {
        return tmux_session(TmuxArgs {
            target: branch,
            prefix: None,
            no_prefix: true,
            session: None,
            prompt_file: None,
            codex_cmd: "codex".to_string(),
            editor_cmd: "nvim".to_string(),
            layout: None,
            attach: true,
            dry_run: false,
            format: args.format,
        });
    }

    if args.path {
        return Ok(format!("{path}\n"));
    }

    format_path_result("search", &branch, &PathBuf::from(path), args.format)
}

/// Enters a running Docker container for the current worktree or starts one.
fn docker_worktree(args: DockerArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    if args.doctor {
        return docker::dx_doctor(&repo, args.service.as_deref());
    }
    docker::dx(&repo, args.service.as_deref(), &args.command)?;
    Ok(String::new())
}

/// Collects worktrees and optionally computes expensive status fields.
///
/// Fast mode avoids per-worktree `git status` and merge-base checks, which keeps
/// `watch` responsive even with many worktrees. In full mode the per-worktree
/// checks are independent Git subprocesses, so they are fanned out across a
/// bounded pool of worker threads to keep wall-clock time low on repositories
/// with many worktrees.
fn collect_listed_worktrees(repo: &Repo, fast: bool) -> Result<Vec<ListedWorktree>> {
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;

    // Load usage metadata once per listing to annotate idle time.
    let metadata = MetadataStore::load(&repo.current_root).unwrap_or_default();

    if fast {
        return Ok(worktrees
            .into_iter()
            .map(|worktree| {
                let path = path_string(&worktree.path);
                let idle_days = metadata.days_since_access(&path);
                let description = metadata.description(&path).map(str::to_string);
                ListedWorktree {
                    worktree,
                    dirty: false,
                    merged: false,
                    changed_files: 0,
                    insertions: 0,
                    deletions: 0,
                    idle_days,
                    description,
                }
            })
            .collect());
    }

    let default_branch = git::default_branch(repo, &worktrees)?;
    compute_status_parallel(repo, worktrees, &default_branch, &metadata)
}

/// Computed status fields for a single worktree in full (non-fast) mode.
#[derive(Clone, Copy, Debug, Default)]
struct WorktreeStatus {
    /// Whether the working tree has changes.
    dirty: bool,
    /// Whether the branch is merged into the default branch.
    merged: bool,
    /// Number of changed paths in the working tree.
    changed_files: usize,
    /// Inserted lines versus `HEAD`.
    insertions: usize,
    /// Deleted lines versus `HEAD`.
    deletions: usize,
}

/// Result of computing a worktree's status fields.
type StatusResult = Result<WorktreeStatus>;

/// Computes status fields for each worktree across worker threads.
///
/// Results are written back into per-index slots so ordering matches the input.
/// The number of workers is capped by both the worktree count and the machine's
/// available parallelism to avoid oversubscribing Git subprocesses.
fn compute_status_parallel(
    repo: &Repo,
    worktrees: Vec<Worktree>,
    default_branch: &str,
    metadata: &MetadataStore,
) -> Result<Vec<ListedWorktree>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let count = worktrees.len();
    if count == 0 {
        return Ok(Vec::new());
    }

    // Per-index result slots; each is filled exactly once by a worker.
    let slots: Vec<Mutex<Option<StatusResult>>> = (0..count).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);

    let available = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let workers = available.min(count).max(1);

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= count {
                    break;
                }
                let worktree = &worktrees[index];
                let result = compute_single_status(repo, worktree, default_branch);
                *slots[index].lock().expect("status slot poisoned") = Some(result);
            });
        }
    });

    let mut listed = Vec::with_capacity(count);
    for (index, worktree) in worktrees.into_iter().enumerate() {
        let status = slots[index]
            .lock()
            .expect("status slot poisoned")
            .take()
            .expect("worker did not fill status slot")?;
        let idle_days = metadata.days_since_access(&path_string(&worktree.path));
        let description = metadata
            .description(&path_string(&worktree.path))
            .map(str::to_string);
        listed.push(ListedWorktree {
            worktree,
            dirty: status.dirty,
            merged: status.merged,
            changed_files: status.changed_files,
            insertions: status.insertions,
            deletions: status.deletions,
            idle_days,
            description,
        });
    }
    Ok(listed)
}

/// Computes the status fields for a single worktree.
///
/// The working-tree summary comes from one `git status --porcelain`; the
/// line-level diff is only requested when the tree is dirty, so clean worktrees
/// pay for a single Git call.
fn compute_single_status(
    repo: &Repo,
    worktree: &Worktree,
    default_branch: &str,
) -> Result<WorktreeStatus> {
    let working = git::working_status(&worktree.path)?;
    let merged = match worktree.branch.as_deref() {
        Some(branch) => git::is_merged(repo, branch, default_branch)?,
        None => false,
    };
    let diff = if working.dirty {
        git::diff_stat(&worktree.path)?
    } else {
        git::DiffStat::default()
    };
    Ok(WorktreeStatus {
        dirty: working.dirty,
        merged,
        changed_files: working.changed_files,
        insertions: diff.insertions,
        deletions: diff.deletions,
    })
}

/// Removes a linked worktree after safety checks.
///
/// Dirty worktrees and branch deletion both require explicit flags.
fn remove_worktree(args: RemoveArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let default_branch = git::default_branch(&repo, &worktrees)?;
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    let wt = git::find_worktree(&worktrees, &target)
        .ok_or_else(|| GatError::NotFound(format!("no worktree found for {}", target)))?;

    if wt.is_primary {
        return Err(GatError::Unsafe(
            "cannot remove the primary worktree".into(),
        ));
    }

    if wt.locked.is_some() && !args.force {
        return Err(GatError::Unsafe(format!(
            "worktree {} is locked; pass --force to ask Git to remove it",
            path_string(&wt.path)
        )));
    }

    let dirty = git::is_dirty(&wt.path)?;
    if dirty && !args.force {
        return Err(GatError::Unsafe(format!(
            "worktree {} has uncommitted changes; pass --force to remove it",
            path_string(&wt.path)
        )));
    }

    let branch = wt.branch.clone();
    if args.delete_branch {
        let branch = branch
            .as_deref()
            .ok_or_else(|| GatError::Unsafe("cannot delete branch for detached worktree".into()))?;
        if branch == default_branch {
            return Err(GatError::Unsafe(format!(
                "cannot delete default branch {branch}"
            )));
        }
        if !args.force_delete_branch && !git::is_merged(&repo, branch, &default_branch)? {
            return Err(GatError::Unsafe(format!(
                "branch {branch} is not merged into {default_branch}; pass --force-delete-branch"
            )));
        }
    }

    if args.dry_run {
        return format_remove_result("would_remove", wt, args.format);
    }

    if !args.yes && !confirm(&format!("Remove worktree {}?", path_string(&wt.path)))? {
        return Ok("Aborted.\n".to_string());
    }

    progress(&format!("removing worktree at {}", path_string(&wt.path)));
    git::remove_worktree(&repo, &wt.path, args.force)?;
    progress("git worktree removed");

    // Clean up metadata
    untrack_worktree(&repo, &wt.path);

    if args.delete_branch {
        if let Some(branch) = branch {
            progress(&format!("deleting branch {branch}"));
            git::delete_branch(&repo, &branch, args.force_delete_branch)?;
        }
    }

    format_remove_result("removed", wt, args.format)
}

/// Merges a ticket branch into the default branch and optionally cleans up.
///
/// Safety model:
/// * The target worktree must exist, be non-primary, and be clean.
/// * The branch cannot be the default branch.
/// * The merge runs in the primary worktree, which must be clean and have the
///   target base branch checked out (the command refuses otherwise rather than
///   silently switching branches).
/// * A merge conflict is aborted so the repository is left unchanged.
/// * Cleanup (remove worktree, delete branch, kill session) only runs after a
///   successful merge and with confirmation.
fn merge_command(args: MergeArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let default_branch = git::default_branch(&repo, &worktrees)?;
    let into = args.into.clone().unwrap_or_else(|| default_branch.clone());
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;

    let wt = git::find_worktree(&worktrees, &target)
        .ok_or_else(|| GatError::NotFound(format!("no worktree found for {target}")))?;

    let branch = wt
        .branch
        .clone()
        .ok_or_else(|| GatError::Unsafe("cannot merge a detached worktree".into()))?;

    // Refuse obviously unsafe merges.
    if branch == into {
        return Err(GatError::Unsafe(format!(
            "cannot merge {branch} into itself"
        )));
    }
    if wt.is_primary {
        return Err(GatError::Unsafe(
            "refusing to merge the primary worktree".into(),
        ));
    }
    if git::is_dirty(&wt.path)? {
        return Err(GatError::Unsafe(format!(
            "worktree {} has uncommitted changes; commit or stash before merging",
            path_string(&wt.path)
        )));
    }

    // The merge happens in the primary worktree; it must be clean and on `into`.
    if git::is_dirty(&repo.primary_root)? {
        return Err(GatError::Unsafe(format!(
            "primary worktree {} is dirty; merge needs a clean base checkout",
            path_string(&repo.primary_root)
        )));
    }
    let primary_branch = git::branch_at(&repo.primary_root)?;
    if primary_branch.as_deref() != Some(into.as_str()) {
        return Err(GatError::Unsafe(format!(
            "primary worktree is on {}, not {into}; check out {into} there first",
            primary_branch.as_deref().unwrap_or("a detached HEAD")
        )));
    }

    // Resolve cleanup flags (--cleanup expands to all three).
    let do_remove = args.remove || args.cleanup;
    let do_delete_branch = args.delete_branch || args.cleanup;
    let do_kill_session = args.kill_session || args.cleanup;

    let already_merged = git::is_merged(&repo, &branch, &into)?;

    if args.dry_run {
        return Ok(format_merge_plan(
            &branch,
            &into,
            &path_string(&wt.path),
            already_merged,
            do_remove,
            do_delete_branch,
            do_kill_session,
            args.format,
        ));
    }

    if !args.yes
        && !confirm(&format!(
            "Merge {branch} into {into}{}?",
            if do_remove { " and clean up" } else { "" }
        ))?
    {
        return Ok("Aborted.\n".to_string());
    }

    // Perform the merge unless it is already an ancestor of `into`.
    if already_merged {
        progress(&format!("{branch} is already merged into {into}"));
    } else {
        progress(&format!("merging {branch} into {into}"));
        match git::merge_branch(&repo.primary_root, &branch, &into, args.no_ff) {
            Ok(output) => {
                if !output.trim().is_empty() {
                    progress(output.trim());
                }
                progress(&format!("merged {branch} into {into}"));
            }
            Err(e) => {
                // Leave the repository clean on conflict.
                git::merge_abort(&repo.primary_root)?;
                return Err(GatError::Git {
                    command: format!("git merge {branch}"),
                    message: format!("{e}\nmerge aborted; resolve conflicts manually"),
                });
            }
        }
    }

    // Cleanup steps (best-effort, after a successful merge).
    let mut cleaned = Vec::new();
    if do_kill_session {
        if let Some(session) = find_existing_session(&target)? {
            if tmux(&["kill-session", "-t", &session]).is_ok() {
                progress(&format!("killed tmux session {session}"));
                cleaned.push(format!("session {session}"));
            }
        }
    }
    if do_remove {
        progress(&format!("removing worktree {}", path_string(&wt.path)));
        git::remove_worktree(&repo, &wt.path, false)?;
        untrack_worktree(&repo, &wt.path);
        cleaned.push("worktree".to_string());

        if do_delete_branch {
            // The branch is merged, so a non-forced delete is safe.
            git::delete_branch(&repo, &branch, false)?;
            progress(&format!("deleted branch {branch}"));
            cleaned.push(format!("branch {branch}"));
        }
    }

    Ok(format_merge_result(&branch, &into, &cleaned, args.format))
}

/// Formats the `gat merge` dry-run plan.
#[allow(clippy::too_many_arguments)]
fn format_merge_plan(
    branch: &str,
    into: &str,
    path: &str,
    already_merged: bool,
    remove: bool,
    delete_branch: bool,
    kill_session: bool,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Text => {
            let mut out = format!(
                "merge plan\nbranch: {branch}\ninto: {into}\nworktree: {path}\nalready merged: {}\n",
                yes_no(already_merged)
            );
            out.push_str(&format!("remove worktree: {}\n", yes_no(remove)));
            out.push_str(&format!("delete branch: {}\n", yes_no(delete_branch)));
            out.push_str(&format!("kill session: {}\n", yes_no(kill_session)));
            out
        }
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"merge_plan\",\"branch\":\"{}\",\"into\":\"{}\",\"path\":\"{}\",\"already_merged\":{},\"remove\":{},\"delete_branch\":{},\"kill_session\":{}}}\n",
            json_escape(branch),
            json_escape(into),
            json_escape(path),
            already_merged,
            remove,
            delete_branch,
            kill_session
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_BRANCH={}\nGAT_INTO={}\nGAT_PATH={}\n",
            shell_escape("merge_plan"),
            shell_escape(branch),
            shell_escape(into),
            shell_escape(path)
        ),
    }
}

/// Formats the `gat merge` success result.
fn format_merge_result(
    branch: &str,
    into: &str,
    cleaned: &[String],
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Text | OutputFormat::Shell => {
            let mut out = format!("merged {branch} into {into}\n");
            if !cleaned.is_empty() {
                out.push_str(&format!("cleaned up: {}\n", cleaned.join(", ")));
            }
            out
        }
        OutputFormat::Json => {
            let cleaned_json = cleaned
                .iter()
                .map(|c| format!("\"{}\"", json_escape(c)))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"status\":\"ok\",\"action\":\"merged\",\"branch\":\"{}\",\"into\":\"{}\",\"cleaned\":[{}]}}\n",
                json_escape(branch),
                json_escape(into),
                cleaned_json
            )
        }
    }
}

/// Prunes stale Git metadata and optionally removes merged worktrees.
fn prune_worktrees(args: PruneArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let stale_output = git::prune_stale(&repo, args.dry_run)?;
    let mut removed = Vec::new();

    // Handle age-based pruning
    if let Some(days) = args.older_than_days {
        let metadata = MetadataStore::load(&repo.current_root).unwrap_or_default();
        let stale_worktrees = metadata.stale_worktrees(days);
        let worktrees = git::list_worktrees(Some(&repo.current_root))?;

        for stale in stale_worktrees {
            // Find the matching worktree
            if let Some(wt) = worktrees
                .iter()
                .find(|w| path_string(&w.path) == stale.path)
            {
                if wt.is_primary {
                    continue;
                }

                // Check if dirty unless --force
                if !args.force && git::is_dirty(&wt.path)? {
                    log::warn!("Skipping dirty worktree: {}", stale.branch);
                    continue;
                }

                if args.dry_run {
                    removed.push((stale.branch.clone(), stale.path.clone()));
                    continue;
                }

                if !args.yes
                    && !confirm(&format!(
                        "Remove worktree {} (unused for {} days)?",
                        stale.branch,
                        metadata.days_since_access(&stale.path).unwrap_or(0)
                    ))?
                {
                    continue;
                }

                git::remove_worktree(&repo, &wt.path, args.force)?;
                untrack_worktree(&repo, &wt.path);
                progress(&format!("removed stale worktree {}", stale.branch));

                removed.push((stale.branch.clone(), stale.path.clone()));
            }
        }
    }

    // Handle merged branch pruning
    if args.merged {
        let worktrees = git::list_worktrees(Some(&repo.current_root))?;
        let default_branch = git::default_branch(&repo, &worktrees)?;
        for wt in worktrees.iter().filter(|wt| !wt.is_primary) {
            let Some(branch) = wt.branch.as_deref() else {
                continue;
            };

            // Skip if already removed by age-based pruning
            let wt_path = path_string(&wt.path);
            if removed.iter().any(|(_, path)| path == &wt_path) {
                continue;
            }

            if !git::is_merged(&repo, branch, &default_branch)? || git::is_dirty(&wt.path)? {
                continue;
            }
            if args.dry_run {
                removed.push((branch.to_string(), wt_path));
                continue;
            }
            if !args.yes && !confirm(&format!("Remove merged worktree {branch}?"))? {
                continue;
            }
            git::remove_worktree(&repo, &wt.path, false)?;
            untrack_worktree(&repo, &wt.path);
            progress(&format!("removed merged worktree {branch}"));

            removed.push((branch.to_string(), wt_path));
        }
    }

    match args.format {
        OutputFormat::Json => {
            let removed_json = removed
                .iter()
                .map(|(branch, path)| {
                    format!(
                        "{{\"branch\":\"{}\",\"path\":\"{}\"}}",
                        json_escape(branch),
                        json_escape(path)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            Ok(format!(
                "{{\"status\":\"ok\",\"dry_run\":{},\"stale_output\":\"{}\",\"removed\":[{}]}}\n",
                args.dry_run,
                json_escape(stale_output.trim()),
                removed_json
            ))
        }
        OutputFormat::Text | OutputFormat::Shell => {
            let mut out = String::new();
            if stale_output.trim().is_empty() {
                out.push_str("No stale worktree metadata.\n");
            } else {
                out.push_str(stale_output.trim());
                out.push('\n');
            }
            for (branch, path) in removed {
                if args.dry_run {
                    out.push_str(&format!("Would remove worktree {branch} @ {path}\n"));
                } else {
                    out.push_str(&format!("Removed worktree {branch} @ {path}\n"));
                }
            }
            Ok(out)
        }
    }
}

/// Creates or switches to an AI-ready tmux session for a worktree.
///
/// The layout is configurable via git config or config file.
/// Default: Codex on the left (55%), prompt editing on the top-right,
/// and a shell on the bottom-right (35%). If already inside tmux, the
/// command switches clients instead of nesting an attached tmux session.
fn tmux_session(args: TmuxArgs) -> Result<String> {
    log::debug!("Starting tmux session for target: {}", args.target);

    let repo = git::discover_repo()?;
    let mut config = config::GatConfig::load(Some(&repo))?;
    // A --layout flag overrides the configured layout for this invocation.
    if let Some(layout) = args.layout.as_deref() {
        config.apply_layout_preset(layout);
    }
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;

    // Override config with CLI args
    let codex_cmd = if args.codex_cmd != "codex" {
        args.codex_cmd.clone()
    } else {
        config.tmux.codex_cmd.clone()
    };

    let editor_cmd = if args.editor_cmd != "nvim" {
        args.editor_cmd.clone()
    } else {
        config.tmux.editor_cmd.clone()
    };

    let plan = build_new_plan(
        &repo,
        &worktrees,
        &NewArgs {
            target: args.target.clone(),
            prefix: args.prefix.clone(),
            no_prefix: args.no_prefix,
            branch: None,
            base: None,
            path: None,
            detach: false,
            description: None,
            template: None,
            dry_run: args.dry_run,
            format: OutputFormat::Text,
        },
    )?;
    // Resolve the worktree description (if any) to fold into the session name.
    let description = {
        let store = MetadataStore::load(&repo.current_root).unwrap_or_default();
        store
            .description(&path_string(&plan.path))
            .map(str::to_string)
    };

    // Prefer an explicit name; otherwise reuse an existing session for the
    // ticket (tolerating description changes), or compute a fresh descriptive
    // name.
    let session = match args.session.clone() {
        Some(name) => name,
        None => find_existing_session(&plan.ticket)?
            .unwrap_or_else(|| session_name(&plan.ticket, description.as_deref())),
    };
    let prompt_file = match args.prompt_file.clone() {
        Some(path) => absolute_from_current(&path)?,
        None => default_prompt_file(&repo, &plan.ticket)?,
    };

    if args.dry_run {
        return Ok(format_tmux_plan(
            &TmuxPlanView {
                plan: &plan,
                session: &session,
                prompt_file: &prompt_file,
                codex_cmd: &codex_cmd,
                editor_cmd: &editor_cmd,
                shell: &config.tmux.shell,
                left_width: config.tmux.left_width,
                bottom_height: config.tmux.bottom_height,
                focus_left: config.tmux.focus_left,
                attach: args.attach,
            },
            args.format,
        ));
    }

    if !command_exists("tmux") {
        return Err(GatError::NotFound("tmux not found on PATH".into()));
    }

    // Use configured shell, fallback to checking common paths
    let shell_path = &config.tmux.shell;
    if !Path::new(shell_path).is_file() {
        return Err(GatError::NotFound(format!("shell {shell_path} not found")));
    }

    if plan.action == NewAction::Create {
        ensure_parent_exists(&plan.path)?;
        progress(&format!(
            "creating worktree for {} on branch {} at {}",
            plan.ticket,
            plan.branch,
            path_string(&plan.path)
        ));
        git::add_worktree(&repo, &plan.path, &plan.branch, &plan.base, plan.detach)?;
        progress(&format!(
            "git worktree created at {}",
            path_string(&plan.path)
        ));
        track_worktree_creation(&repo, &plan.path, &plan.branch);
    } else {
        track_worktree_access(&repo, &plan.path);
    }

    ensure_prompt_file(&prompt_file, &plan.ticket, &plan.path)?;

    progress(&format!("preparing tmux session {session}"));
    launch_tmux_session(&TmuxLaunch {
        session: &session,
        worktree_path: &plan.path,
        prompt_file: &prompt_file,
        codex_cmd: &codex_cmd,
        editor_cmd: &editor_cmd,
        config: &config,
        attach: args.attach,
    })?;

    set_session_metadata(&session, &plan.path, &plan.branch, description.as_deref());

    if args.attach {
        Ok(String::new())
    } else {
        Ok(format_tmux_ready(
            &plan,
            &session,
            &prompt_file,
            args.format,
        ))
    }
}

/// Attaches to (or creates) the tmux session for an existing worktree.
///
/// Unlike `gat tmux`, this never creates a worktree. Resolution order:
/// 1. If a tmux session for the ticket already exists, attach/switch to it.
/// 2. Otherwise, if the worktree exists, build the gat layout and attach.
/// 3. Otherwise, error: there is nothing to switch to.
fn switch_session(args: SwitchArgs) -> Result<String> {
    log::debug!("Switching to session for target: {}", args.target);

    let repo = git::discover_repo()?;
    let mut config = config::GatConfig::load(Some(&repo))?;
    // A --layout flag overrides the configured layout for this invocation.
    if let Some(layout) = args.layout.as_deref() {
        config.apply_layout_preset(layout);
    }
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;

    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;

    // Override config defaults with explicit CLI args.
    let codex_cmd = if args.codex_cmd != "codex" {
        args.codex_cmd.clone()
    } else {
        config.tmux.codex_cmd.clone()
    };
    let editor_cmd = if args.editor_cmd != "nvim" {
        args.editor_cmd.clone()
    } else {
        config.tmux.editor_cmd.clone()
    };

    // Locate the existing worktree, if any.
    let worktree = git::find_worktree(&worktrees, &target);

    // Resolve description (folded into the session name for at-a-glance info).
    let description = worktree.and_then(|wt| {
        let store = MetadataStore::load(&repo.current_root).unwrap_or_default();
        store
            .description(&path_string(&wt.path))
            .map(str::to_string)
    });

    // Resolve the session name. An explicit --session wins; otherwise reuse a
    // live session for the ticket, falling back to a fresh descriptive name.
    let existing = if args.session.is_some() {
        None
    } else {
        find_existing_session(&target)?
    };
    let session = match (args.session.clone(), existing.clone()) {
        (Some(name), _) => name,
        (None, Some(name)) => name,
        (None, None) => session_name(&target, description.as_deref()),
    };

    // A session already exists if we found one, or the explicit name is live.
    let session_exists =
        existing.is_some() || (command_exists("tmux") && tmux_has_session(&session)?);

    if args.dry_run {
        return Ok(format_switch_plan(
            &target,
            &session,
            worktree.map(|wt| wt.path.as_path()),
            session_exists,
            args.format,
        ));
    }

    // Case 1: a live session exists; attach regardless of worktree presence.
    if session_exists {
        if let Some(wt) = worktree {
            track_worktree_access(&repo, &wt.path);
        }
        progress(&format!("attaching to tmux session {session}"));
        if args.attach {
            if env::var_os("TMUX").is_some() {
                tmux(&["switch-client", "-t", &session])?;
            } else {
                tmux(&["attach-session", "-t", &session])?;
            }
            return Ok(String::new());
        }
        return Ok(format_switch_ready(
            "attached",
            &target,
            &session,
            args.format,
        ));
    }

    // Case 2: no session, but the worktree exists; build the layout.
    let Some(wt) = worktree else {
        // Case 3: nothing to switch to.
        return Err(GatError::NotFound(format!(
            "no worktree found for {target}; create it first with `gat new {}`",
            args.target
        )));
    };

    track_worktree_access(&repo, &wt.path);

    let prompt_file = match args.prompt_file.clone() {
        Some(path) => absolute_from_current(&path)?,
        None => default_prompt_file(&repo, &target)?,
    };
    ensure_prompt_file(&prompt_file, &target, &wt.path)?;

    progress(&format!("opening tmux session {session}"));
    launch_tmux_session(&TmuxLaunch {
        session: &session,
        worktree_path: &wt.path,
        prompt_file: &prompt_file,
        codex_cmd: &codex_cmd,
        editor_cmd: &editor_cmd,
        config: &config,
        attach: args.attach,
    })?;

    let branch = wt.branch.clone().unwrap_or_else(|| target.clone());
    set_session_metadata(&session, &wt.path, &branch, description.as_deref());

    if args.attach {
        Ok(String::new())
    } else {
        Ok(format_switch_ready(
            "created",
            &target,
            &session,
            args.format,
        ))
    }
}

/// Formats the `gat switch` dry-run plan.
fn format_switch_plan(
    target: &str,
    session: &str,
    worktree_path: Option<&Path>,
    session_exists: bool,
    format: OutputFormat,
) -> String {
    let action = if session_exists {
        "attach_existing"
    } else if worktree_path.is_some() {
        "create_session"
    } else {
        "no_worktree"
    };
    let path_display = worktree_path
        .map(path_string)
        .unwrap_or_else(|| "<none>".to_string());
    match format {
        OutputFormat::Text => format!(
            "switch plan for {target}\nsession: {session}\nsession exists: {}\nworktree: {path_display}\naction: {action}\n",
            yes_no(session_exists)
        ),
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"switch_plan\",\"target\":\"{}\",\"session\":\"{}\",\"session_exists\":{},\"worktree\":{},\"resolved_action\":\"{}\"}}\n",
            json_escape(target),
            json_escape(session),
            session_exists,
            worktree_path.map(|p| format!("\"{}\"", json_escape(&path_string(p)))).unwrap_or_else(|| "null".to_string()),
            action
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_TARGET={}\nGAT_SESSION={}\nGAT_SESSION_EXISTS={}\nGAT_PATH={}\n",
            shell_escape("switch_plan"),
            shell_escape(target),
            shell_escape(session),
            shell_escape(&session_exists.to_string()),
            shell_escape(&path_display)
        ),
    }
}

/// Formats the `gat switch` non-attach result.
fn format_switch_ready(action: &str, target: &str, session: &str, format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => format!("tmux session {session} {action} for {target}\n"),
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"switch_{}\",\"target\":\"{}\",\"session\":\"{}\"}}\n",
            json_escape(action),
            json_escape(target),
            json_escape(session)
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_TARGET={}\nGAT_SESSION={}\n",
            shell_escape(&format!("switch_{action}")),
            shell_escape(target),
            shell_escape(session)
        ),
    }
}

/// Sets or shows a worktree's description.
///
/// With no description argument, the current description is printed. With a
/// non-empty value, it is stored; an empty value clears it. The target worktree
/// must exist so descriptions stay anchored to real worktrees.
fn describe_worktree(args: DescribeArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    let wt = git::find_worktree(&worktrees, &target)
        .ok_or_else(|| GatError::NotFound(format!("no worktree found for {target}")))?;
    let path = path_string(&wt.path);
    let branch = wt.branch.clone().unwrap_or_else(|| target.clone());

    match args.description {
        // Read-only: print the current description.
        None => {
            let store = MetadataStore::load(&repo.current_root).unwrap_or_default();
            let current = store.description(&path).unwrap_or("");
            Ok(format_describe_result(
                &target,
                &path,
                current,
                false,
                args.format,
            ))
        }
        // Mutate: set or clear the description.
        Some(description) => {
            let trimmed = description.trim().to_string();
            MetadataStore::update(&repo.current_root, |m| {
                m.set_description(&path, &branch, &trimmed);
            })?;
            let cleared = trimmed.is_empty();
            progress(&format!(
                "{} description for {target}",
                if cleared { "cleared" } else { "set" }
            ));
            Ok(format_describe_result(
                &target,
                &path,
                &trimmed,
                true,
                args.format,
            ))
        }
    }
}

/// Formats `gat describe` output for text, JSON, and shell modes.
fn format_describe_result(
    target: &str,
    path: &str,
    description: &str,
    updated: bool,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Text => {
            if description.is_empty() {
                format!("{target} has no description\n")
            } else {
                format!("{target}: {description}\n")
            }
        }
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"describe\",\"target\":\"{}\",\"path\":\"{}\",\"description\":{},\"updated\":{}}}\n",
            json_escape(target),
            json_escape(path),
            if description.is_empty() {
                "null".to_string()
            } else {
                format!("\"{}\"", json_escape(description))
            },
            updated
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_TARGET={}\nGAT_PATH={}\nGAT_DESCRIPTION={}\n",
            shell_escape("describe"),
            shell_escape(target),
            shell_escape(path),
            shell_escape(description)
        ),
    }
}

/// A live gat-managed tmux session and its recorded worktree metadata.
#[derive(Debug)]
struct GatSession {
    /// tmux session name.
    name: String,
    /// Whether a client is currently attached.
    attached: bool,
    /// Number of windows in the session.
    windows: usize,
    /// Worktree branch recorded via the `@gat_branch` option, if any.
    branch: Option<String>,
    /// Worktree path recorded via the `@gat_path` option, if any.
    path: Option<String>,
    /// Worktree description recorded via the `@gat_description` option, if any.
    description: Option<String>,
}

/// Lists live gat-managed tmux sessions with their recorded worktree metadata.
///
/// Only sessions whose names start with the `gat-` prefix are reported, so this
/// never lists unrelated tmux sessions. Each session's `@gat_branch`,
/// `@gat_path`, and `@gat_description` options (set when the session is created)
/// provide at-a-glance context.
fn list_sessions(args: SessionsArgs) -> Result<String> {
    if !command_exists("tmux") {
        return Err(GatError::NotFound("tmux not found on PATH".into()));
    }

    // Pull one line per session with the fields we care about, tab-separated.
    // Custom @gat_* options render empty when unset, which we treat as None.
    let format = "#{session_name}\t#{session_attached}\t#{session_windows}\t#{@gat_branch}\t#{@gat_path}\t#{@gat_description}";
    let output = ProcessCommand::new("tmux")
        .args(["list-sessions", "-F", format])
        .output()?;

    // tmux exits non-zero when no server is running; treat that as "no sessions".
    if !output.status.success() {
        return Ok(empty_sessions_output(args.format));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();
    for line in stdout.lines() {
        if let Some(session) = parse_session_line(line) {
            sessions.push(session);
        }
    }

    // Stable ordering by session name keeps output deterministic.
    sessions.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(format_sessions(&sessions, args.format))
}

/// Parses one tab-separated `tmux list-sessions` line into a [`GatSession`].
///
/// Returns `None` for sessions that are not gat-managed (name does not start
/// with `gat-`).
fn parse_session_line(line: &str) -> Option<GatSession> {
    let mut fields = line.split('\t');
    let name = fields.next()?.trim().to_string();
    if !name.starts_with("gat-") {
        return None;
    }
    let attached = fields.next().map(|v| v.trim() != "0").unwrap_or(false);
    let windows = fields
        .next()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let branch = non_empty(fields.next());
    let path = non_empty(fields.next());
    let description = non_empty(fields.next());

    Some(GatSession {
        name,
        attached,
        windows,
        branch,
        path,
        description,
    })
}

/// Returns the trimmed value as `Some` unless it is missing or empty.
fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

/// Formats the empty-session result for each output mode.
fn empty_sessions_output(format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => "[]\n".to_string(),
        OutputFormat::Text | OutputFormat::Shell => "No gat tmux sessions.\n".to_string(),
    }
}

/// Formats live gat sessions for text, JSON, and shell modes.
fn format_sessions(sessions: &[GatSession], format: OutputFormat) -> String {
    if sessions.is_empty() {
        return empty_sessions_output(format);
    }
    match format {
        OutputFormat::Text => {
            let mut out = String::from(
                "Session                              A  Win  Branch          Description / Path\n",
            );
            for s in sessions {
                let attached = if s.attached { "*" } else { " " };
                let branch = s.branch.as_deref().unwrap_or("-");
                let info = s
                    .description
                    .as_deref()
                    .or(s.path.as_deref())
                    .unwrap_or("-");
                out.push_str(&format!(
                    "{:<36} {} {:>4}  {:<15} {}\n",
                    s.name, attached, s.windows, branch, info
                ));
            }
            out
        }
        OutputFormat::Json => {
            let entries = sessions
                .iter()
                .map(|s| {
                    format!(
                        "{{\"name\":\"{}\",\"attached\":{},\"windows\":{},\"branch\":{},\"path\":{},\"description\":{}}}",
                        json_escape(&s.name),
                        s.attached,
                        s.windows,
                        option_json(s.branch.as_deref()),
                        option_json(s.path.as_deref()),
                        option_json(s.description.as_deref())
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("[{entries}]\n")
        }
        OutputFormat::Shell => {
            let mut out = String::new();
            for s in sessions {
                out.push_str(&format!("GAT_SESSION={}\n", shell_escape(&s.name)));
            }
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive TUI support
// ---------------------------------------------------------------------------

/// A worktree row presented in the interactive TUI.
///
/// This is a flattened, owned snapshot so the TUI never holds borrowed Git
/// state across its event loop.
#[cfg(feature = "tui")]
#[derive(Clone, Debug)]
pub(crate) struct UiWorktreeRow {
    /// Branch name, or a `detached:<hash>` label.
    pub branch: String,
    /// Worktree path as a display string.
    pub path: String,
    /// Whether this is the primary worktree.
    pub is_primary: bool,
    /// Whether the working tree has uncommitted changes.
    pub dirty: bool,
    /// Whether the branch is merged into the default branch.
    pub merged: bool,
    /// Number of changed paths in the working tree.
    pub changed_files: usize,
    /// Inserted lines versus HEAD.
    pub insertions: usize,
    /// Deleted lines versus HEAD.
    pub deletions: usize,
    /// Days since last access, if known.
    pub idle_days: Option<u64>,
    /// Optional worktree description.
    pub description: Option<String>,
}

/// A point-in-time snapshot of repository state for the TUI.
#[cfg(feature = "tui")]
#[derive(Clone, Debug)]
pub(crate) struct UiSnapshot {
    /// Repository display name.
    pub repo_name: String,
    /// Worktree rows.
    pub worktrees: Vec<UiWorktreeRow>,
    /// Live gat tmux sessions (empty when tmux is unavailable).
    pub sessions: Vec<UiSessionRow>,
}

/// A tmux session row presented in the interactive TUI.
#[cfg(feature = "tui")]
#[derive(Clone, Debug)]
pub(crate) struct UiSessionRow {
    /// tmux session name.
    pub name: String,
    /// Whether a client is attached.
    pub attached: bool,
    /// Number of windows.
    pub windows: usize,
    /// Recorded branch, if any.
    pub branch: Option<String>,
    /// Recorded description, if any.
    pub description: Option<String>,
}

/// Builds a fresh [`UiSnapshot`] from the current repository state.
///
/// Reuses the same listing and session-enumeration logic as `gat list` and
/// `gat sessions`, so the TUI shows exactly what those commands would.
#[cfg(feature = "tui")]
pub(crate) fn ui_snapshot(fast: bool) -> Result<UiSnapshot> {
    let repo = git::discover_repo()?;
    let listed = collect_listed_worktrees(&repo, fast)?;

    let worktrees = listed
        .into_iter()
        .map(|item| {
            let branch = item.worktree.branch.clone().unwrap_or_else(|| {
                format!(
                    "detached:{}",
                    git::short_head(item.worktree.head.as_deref())
                )
            });
            UiWorktreeRow {
                branch,
                path: path_string(&item.worktree.path),
                is_primary: item.worktree.is_primary,
                dirty: item.dirty,
                merged: item.merged,
                changed_files: item.changed_files,
                insertions: item.insertions,
                deletions: item.deletions,
                idle_days: item.idle_days,
                description: item.description,
            }
        })
        .collect();

    // Sessions are best-effort: a missing tmux must not break the dashboard.
    let sessions = ui_collect_sessions().unwrap_or_default();

    Ok(UiSnapshot {
        repo_name: repo.repo_name,
        worktrees,
        sessions,
    })
}

/// Collects live gat sessions as TUI rows, or an empty list when unavailable.
#[cfg(feature = "tui")]
fn ui_collect_sessions() -> Result<Vec<UiSessionRow>> {
    if !command_exists("tmux") {
        return Ok(Vec::new());
    }
    let format = "#{session_name}\t#{session_attached}\t#{session_windows}\t#{@gat_branch}\t#{@gat_path}\t#{@gat_description}";
    let output = ProcessCommand::new("tmux")
        .args(["list-sessions", "-F", format])
        .output()?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut rows: Vec<UiSessionRow> = stdout
        .lines()
        .filter_map(parse_session_line)
        .map(|s| UiSessionRow {
            name: s.name,
            attached: s.attached,
            windows: s.windows,
            branch: s.branch,
            description: s.description,
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rows)
}

/// Switches to (or opens) the tmux session for a ticket from the TUI.
///
/// Thin wrapper over [`switch_session`] with TUI-friendly defaults so the event
/// loop does not need to construct CLI argument structs.
#[cfg(feature = "tui")]
pub(crate) fn ui_switch(target: &str) -> Result<String> {
    switch_session(SwitchArgs {
        target: target.to_string(),
        prefix: None,
        no_prefix: true,
        session: None,
        prompt_file: None,
        codex_cmd: "codex".to_string(),
        editor_cmd: "nvim".to_string(),
        layout: None,
        attach: true,
        dry_run: false,
        format: OutputFormat::Text,
    })
}

/// Sets a worktree description from the TUI.
#[cfg(feature = "tui")]
pub(crate) fn ui_set_description(target: &str, description: &str) -> Result<String> {
    describe_worktree(DescribeArgs {
        target: target.to_string(),
        prefix: None,
        no_prefix: true,
        description: Some(description.to_string()),
        format: OutputFormat::Text,
    })
}

/// Removes a worktree from the TUI (non-forced, auto-confirmed).
#[cfg(feature = "tui")]
pub(crate) fn ui_remove(target: &str) -> Result<String> {
    remove_worktree(RemoveArgs {
        target: target.to_string(),
        prefix: None,
        no_prefix: true,
        force: false,
        yes: true,
        dry_run: false,
        delete_branch: false,
        force_delete_branch: false,
        format: OutputFormat::Text,
    })
}

/// Inspect or edit gat configuration via `gat config`.
///
/// Reads the effective configuration (file + git + env) for `get`/`list`, but
/// `set`/`init` only ever write the on-disk config file so the change is
/// explicit and persistent.
fn config_command(args: ConfigArgs) -> Result<String> {
    match args.action {
        ConfigAction::Path => {
            let path = config::config_file_path()?;
            Ok(format!("{}\n", path_string(&path)))
        }
        ConfigAction::Init { force } => {
            let path = config::config_file_path()?;
            if path.exists() && !force {
                return Err(GatError::Unsafe(format!(
                    "config file already exists at {}; pass --force to overwrite",
                    path_string(&path)
                )));
            }
            // Write defaults (the repo context is irrelevant for a fresh file).
            let written = config::GatConfig::default().save()?;
            progress(&format!("wrote config to {}", path_string(&written)));
            Ok(format!("Initialized config at {}\n", path_string(&written)))
        }
        ConfigAction::Get { key } => {
            let repo = git::discover_repo().ok();
            let config = config::GatConfig::load(repo.as_ref())?;
            match config.get_key(&key) {
                Some(value) => Ok(format!("{value}\n")),
                None => Err(GatError::NotFound(format!("config key '{key}' is not set"))),
            }
        }
        ConfigAction::Set { key, value } => {
            // Mutate only the on-disk file so the change is persistent and does
            // not silently inherit git/env overrides.
            let mut file_config = config::GatConfig::load_from_file_public()?;
            file_config.set_key(&key, &value)?;
            let path = file_config.save()?;
            progress(&format!("set {key} in {}", path_string(&path)));
            Ok(format!("{key} = {value}\n"))
        }
        ConfigAction::List => {
            let repo = git::discover_repo().ok();
            let config = config::GatConfig::load(repo.as_ref())?;
            match args.format {
                OutputFormat::Json => {
                    let entries = config
                        .entries()
                        .iter()
                        .map(|(k, v)| format!("\"{}\":\"{}\"", json_escape(k), json_escape(v)))
                        .collect::<Vec<_>>()
                        .join(",");
                    Ok(format!("{{{entries}}}\n"))
                }
                OutputFormat::Text | OutputFormat::Shell => {
                    let mut out = String::new();
                    for (k, v) in config.entries() {
                        out.push_str(&format!("{k} = {v}\n"));
                    }
                    Ok(out)
                }
            }
        }
    }
}

/// Entry point for `gat ui`.
///
/// When the `tui` feature is enabled this launches the interactive dashboard;
/// otherwise it returns an actionable error explaining how to enable it.
#[cfg(feature = "tui")]
fn run_ui(args: UiArgs) -> Result<String> {
    crate::tui::run(args.fast)
}

/// Stub for `gat ui` when the `tui` feature is not compiled in.
#[cfg(not(feature = "tui"))]
fn run_ui(args: UiArgs) -> Result<String> {
    // Touch the field so the no-tui build does not warn about it being unread.
    let _ = args.fast;
    Err(GatError::NotFound(
        "the interactive TUI is not available in this build; reinstall with `cargo install --path . --features tui`".into(),
    ))
}

/// Resolved inputs for creating or attaching a gat tmux session.
struct TmuxLaunch<'a> {
    /// tmux session name.
    session: &'a str,
    /// Worktree root used as the working directory for all panes.
    worktree_path: &'a Path,
    /// Prompt draft file exported to panes.
    prompt_file: &'a Path,
    /// Command launched in the AI pane.
    codex_cmd: &'a str,
    /// Command launched in the editor pane.
    editor_cmd: &'a str,
    /// Effective configuration (layout percentages, shell, focus).
    config: &'a config::GatConfig,
    /// Whether to attach/switch to the session after ensuring it exists.
    attach: bool,
}

/// Creates the gat tmux layout if the session is absent, then optionally attaches.
///
/// Splitting this out lets both `gat tmux` and `gat switch` share one
/// implementation of the pane layout and attach/switch behavior. The caller is
/// responsible for ensuring the worktree and prompt file already exist.
fn launch_tmux_session(launch: &TmuxLaunch) -> Result<()> {
    let TmuxLaunch {
        session,
        worktree_path,
        prompt_file,
        codex_cmd,
        editor_cmd,
        config,
        attach,
    } = *launch;

    if !command_exists("tmux") {
        return Err(GatError::NotFound("tmux not found on PATH".into()));
    }

    let shell_path = &config.tmux.shell;
    if !Path::new(shell_path).is_file() {
        return Err(GatError::NotFound(format!("shell {shell_path} not found")));
    }

    if !tmux_has_session(session)? {
        log::info!("Creating new tmux session: {}", session);
        let worktree_path = path_string(worktree_path);
        let prompt_path = path_string(prompt_file);

        // Create base session
        let left_pane_output = tmux_output(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-s",
            session,
            "-n",
            "gat",
            "-c",
            &worktree_path,
            shell_path,
        ])?;
        let left_pane = tmux_pane_id(&left_pane_output, "new-session")?;

        // Split horizontally - right pane width = 100 - left_width
        let right_width = 100 - config.tmux.left_width;
        let right_top_pane_output = tmux_output(&[
            "split-window",
            "-h",
            "-l",
            &format!("{}%", right_width),
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            left_pane,
            "-c",
            &worktree_path,
            shell_path,
        ])?;
        let right_top_pane = tmux_pane_id(&right_top_pane_output, "split-window -h")?;

        // Split right pane vertically - bottom height = configured percentage
        let right_bottom_pane_output = tmux_output(&[
            "split-window",
            "-v",
            "-l",
            &format!("{}%", config.tmux.bottom_height),
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            right_top_pane,
            "-c",
            &worktree_path,
            shell_path,
        ])?;
        let _right_bottom_pane = tmux_pane_id(&right_bottom_pane_output, "split-window -v")?;

        // Send commands to panes
        tmux(&[
            "send-keys",
            "-t",
            left_pane,
            &format!(
                "GAT_PROMPT_FILE={} {}",
                shell_escape(&prompt_path),
                codex_cmd
            ),
            "C-m",
        ])?;
        tmux(&[
            "send-keys",
            "-t",
            right_top_pane,
            &format!(
                "GAT_PROMPT_FILE={} {} {}",
                shell_escape(&prompt_path),
                editor_cmd,
                shell_escape(&worktree_path)
            ),
            "C-m",
        ])?;

        // Focus left pane if configured
        if config.tmux.focus_left {
            tmux(&["select-pane", "-t", left_pane])?;
        } else {
            tmux(&["select-pane", "-t", right_top_pane])?;
        }
        progress(&format!("tmux session {session} created with 3 panes"));
    }

    if attach {
        if env::var_os("TMUX").is_some() {
            tmux(&["switch-client", "-t", session])?;
        } else {
            tmux(&["attach-session", "-t", session])?;
        }
    }

    Ok(())
}

/// Archives a linked worktree by moving it with `git worktree move`.
///
/// Using Git's move command preserves metadata and makes archive operations
/// reversible; this is safer than unregistering the worktree and moving files
/// manually.
fn archive_worktree(args: ArchiveArgs) -> Result<String> {
    let repo = git::discover_repo()?;
    let worktrees = git::list_worktrees(Some(&repo.current_root))?;
    let target = normalize_target(&repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    let wt = git::find_worktree(&worktrees, &target)
        .ok_or_else(|| GatError::NotFound(format!("no worktree found for {target}")))?;

    if wt.is_primary {
        return Err(GatError::Unsafe(
            "cannot archive the primary worktree".into(),
        ));
    }

    if git::is_dirty(&wt.path)? && !args.force {
        return Err(GatError::Unsafe(format!(
            "worktree {} has uncommitted changes; pass --force to archive it",
            path_string(&wt.path)
        )));
    }

    let archive_root = args.archive_dir.unwrap_or_else(|| {
        repo.repo_parent.join(format!(
            "{}-archive",
            sanitize_path_component(&repo.repo_name)
        ))
    });
    let archive_root = absolute_from_current(&archive_root)?;

    // Validate archive directory early
    if archive_root.exists() {
        if !archive_root.is_dir() {
            return Err(GatError::Unsafe(format!(
                "archive path exists but is not a directory: {}",
                path_string(&archive_root)
            )));
        }

        // Check if writable by trying to create a test file
        let test_file = archive_root.join(".gat_write_test");
        if let Err(e) = std::fs::write(&test_file, b"test") {
            return Err(GatError::Unsafe(format!(
                "archive directory is not writable: {} ({})",
                path_string(&archive_root),
                e
            )));
        }
        let _ = std::fs::remove_file(&test_file);
    } else {
        // Ensure parent exists and is writable
        if let Some(parent) = archive_root.parent() {
            if !parent.exists() {
                return Err(GatError::Unsafe(format!(
                    "archive parent directory does not exist: {}",
                    path_string(parent)
                )));
            }
        }
    }

    let archive_path = unique_archive_path(
        &archive_root,
        wt.branch.as_deref().unwrap_or_else(|| {
            wt.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("detached")
        }),
    );

    if args.dry_run {
        return format_archive_result("would_archive", wt, &archive_path, args.format);
    }

    if !args.yes
        && !confirm(&format!(
            "Archive worktree {} to {}?",
            path_string(&wt.path),
            path_string(&archive_path)
        ))?
    {
        return Ok("Aborted.\n".to_string());
    }

    ensure_parent_exists(&archive_path)?;
    progress(&format!(
        "archiving worktree {} to {}",
        path_string(&wt.path),
        path_string(&archive_path)
    ));
    git::move_worktree(&repo, &wt.path, &archive_path)?;
    progress("git worktree moved to archive");

    // Update metadata: the worktree moved, so migrate its entry to the new path.
    let old_path = path_string(&wt.path);
    let new_path = path_string(&archive_path);
    let branch = wt.branch.clone().unwrap_or_else(|| "archived".to_string());
    if let Err(e) = MetadataStore::update(&repo.current_root, |m| {
        m.remove(&old_path);
        m.track_creation(&new_path, &branch);
    }) {
        log::warn!("Failed to update worktree metadata after archive: {e}");
    }

    format_archive_result("archived", wt, &archive_path, args.format)
}

/// Creates the tmux prompt file if it does not already exist.
fn ensure_prompt_file(path: &Path, ticket: &str, worktree_path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        std::fs::write(
            path,
            format!(
                "# Pre-prompt for {ticket}\n\nWorktree: {}\n\nGoal:\n\nContext:\n\nCommands:\n\n",
                path_string(worktree_path)
            ),
        )?;
    }
    Ok(())
}

/// Computes the default prompt draft path without writing inside the worktree.
fn default_prompt_file(repo: &Repo, ticket: &str) -> Result<PathBuf> {
    let repo_key = format!(
        "{}-{}",
        sanitize_path_component(&repo.repo_name),
        stable_hex_hash(&path_string(&repo.primary_root))
    );
    Ok(gat_config_root()?
        .join("worktrees")
        .join(repo_key)
        .join(sanitize_path_component(ticket))
        .join("pre-prompt.md"))
}

/// Returns the configuration root used by `gat`.
fn gat_config_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("gat"));
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join(".config").join("gat"));
    }
    Err(GatError::NotFound(
        "HOME is not set; cannot resolve gat config directory".into(),
    ))
}

/// Produces a stable short hash for namespacing config by repository path.
fn stable_hex_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Grouped inputs for rendering a tmux dry-run plan.
///
/// Bundling these avoids an unwieldy positional argument list and keeps the
/// text, JSON, and shell renderers in sync.
struct TmuxPlanView<'a> {
    /// Worktree creation/reuse plan.
    plan: &'a NewPlan,
    /// Resolved tmux session name.
    session: &'a str,
    /// Prompt draft file path exported to panes.
    prompt_file: &'a Path,
    /// Command launched in the AI pane.
    codex_cmd: &'a str,
    /// Command launched in the editor pane.
    editor_cmd: &'a str,
    /// Shell used for tmux panes.
    shell: &'a str,
    /// Left pane width percentage.
    left_width: u8,
    /// Bottom-right pane height percentage.
    bottom_height: u8,
    /// Whether the left (AI) pane is focused on creation.
    focus_left: bool,
    /// Whether the session would be attached/switched to.
    attach: bool,
}

/// Formats a dry-run tmux plan.
fn format_tmux_plan(view: &TmuxPlanView, format: OutputFormat) -> String {
    let TmuxPlanView {
        plan,
        session,
        prompt_file,
        codex_cmd,
        editor_cmd,
        shell,
        left_width,
        bottom_height,
        focus_left,
        attach,
    } = *view;
    let attach_cmd = tmux_attach_command(session, attach);
    let right_width = 100u8.saturating_sub(left_width);
    let focus = if focus_left { "ai" } else { "editor" };
    match format {
        OutputFormat::Text => format!(
            "tmux plan for {} @ {}\nworktree action: {}\nsession: {}\nprompt file: {}\nshell: {}\nleft pane: {} ({}%)\nright top pane: {} {} ({}%)\nright bottom pane: shell ({}%)\nfocus: {}\nattach: {}\n",
            plan.ticket,
            path_string(&plan.path),
            new_action_label(&plan.action),
            session,
            path_string(prompt_file),
            shell,
            codex_cmd,
            left_width,
            editor_cmd,
            shell_escape(&path_string(&plan.path)),
            right_width,
            bottom_height,
            focus,
            attach_cmd
        ),
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"tmux_plan\",\"ticket\":\"{}\",\"branch\":\"{}\",\"path\":\"{}\",\"worktree_action\":\"{}\",\"session\":\"{}\",\"prompt_file\":\"{}\",\"shell\":\"{}\",\"codex_cmd\":\"{}\",\"editor_cmd\":\"{}\",\"editor_target\":\"{}\",\"left_width\":{},\"bottom_height\":{},\"focus_left\":{},\"attach_command\":\"{}\"}}\n",
            json_escape(&plan.ticket),
            json_escape(&plan.branch),
            json_escape(&path_string(&plan.path)),
            new_action_label(&plan.action),
            json_escape(session),
            json_escape(&path_string(prompt_file)),
            json_escape(shell),
            json_escape(codex_cmd),
            json_escape(editor_cmd),
            json_escape(&path_string(&plan.path)),
            left_width,
            bottom_height,
            focus_left,
            json_escape(&attach_cmd)
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_TICKET={}\nGAT_BRANCH={}\nGAT_PATH={}\nGAT_SESSION={}\nGAT_PROMPT_FILE={}\nGAT_SHELL={}\nGAT_CODEX_CMD={}\nGAT_EDITOR_CMD={}\nGAT_EDITOR_TARGET={}\nGAT_LEFT_WIDTH={}\nGAT_BOTTOM_HEIGHT={}\nGAT_FOCUS_LEFT={}\nGAT_ATTACH_CMD={}\n",
            shell_escape("tmux_plan"),
            shell_escape(&plan.ticket),
            shell_escape(&plan.branch),
            shell_escape(&path_string(&plan.path)),
            shell_escape(session),
            shell_escape(&path_string(prompt_file)),
            shell_escape(shell),
            shell_escape(codex_cmd),
            shell_escape(editor_cmd),
            shell_escape(&path_string(&plan.path)),
            shell_escape(&left_width.to_string()),
            shell_escape(&bottom_height.to_string()),
            shell_escape(&focus_left.to_string()),
            shell_escape(&attach_cmd)
        ),
    }
}

/// Formats a created or reused tmux session result.
fn format_tmux_ready(
    plan: &NewPlan,
    session: &str,
    prompt_file: &Path,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Text => format!(
            "tmux session {session} ready for {} @ {}\n",
            plan.ticket,
            path_string(&plan.path)
        ),
        OutputFormat::Json => format!(
            "{{\"status\":\"ok\",\"action\":\"tmux_ready\",\"ticket\":\"{}\",\"branch\":\"{}\",\"path\":\"{}\",\"worktree_action\":\"{}\",\"session\":\"{}\",\"prompt_file\":\"{}\"}}\n",
            json_escape(&plan.ticket),
            json_escape(&plan.branch),
            json_escape(&path_string(&plan.path)),
            new_action_label(&plan.action),
            json_escape(session),
            json_escape(&path_string(prompt_file))
        ),
        OutputFormat::Shell => format!(
            "GAT_ACTION={}\nGAT_TICKET={}\nGAT_BRANCH={}\nGAT_PATH={}\nGAT_SESSION={}\nGAT_PROMPT_FILE={}\n",
            shell_escape("tmux_ready"),
            shell_escape(&plan.ticket),
            shell_escape(&plan.branch),
            shell_escape(&path_string(&plan.path)),
            shell_escape(session),
            shell_escape(&path_string(prompt_file))
        ),
    }
}

/// Builds the attach/switch command that tmux mode would run.
fn tmux_attach_command(session: &str, attach: bool) -> String {
    if !attach {
        return "no attach".to_string();
    }
    if env::var_os("TMUX").is_some() {
        format!("tmux switch-client -t {}", shell_escape(session))
    } else {
        format!("tmux attach-session -t {}", shell_escape(session))
    }
}

/// Returns a stable machine-facing label for a worktree creation action.
fn new_action_label(action: &NewAction) -> &'static str {
    match action {
        NewAction::Create => "create",
        NewAction::Existing => "existing",
        NewAction::DryRun => "dry_run",
    }
}

/// Formats archive output for text, shell, and JSON modes.
fn format_archive_result(
    action: &str,
    wt: &Worktree,
    archive_path: &Path,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Text | OutputFormat::Shell => Ok(format!(
            "{action} worktree {} from {} to {}\n",
            wt.branch.as_deref().unwrap_or("<detached>"),
            path_string(&wt.path),
            path_string(archive_path)
        )),
        OutputFormat::Json => Ok(format!(
            "{{\"status\":\"ok\",\"action\":\"{}\",\"branch\":{},\"from\":\"{}\",\"to\":\"{}\"}}\n",
            json_escape(action),
            option_json(wt.branch.as_deref()),
            json_escape(&path_string(&wt.path)),
            json_escape(&path_string(archive_path))
        )),
    }
}

/// Chooses a non-conflicting archive destination below `root`.
fn unique_archive_path(root: &Path, name: &str) -> PathBuf {
    let safe_name = sanitize_path_component(name);
    let mut candidate = root.join(&safe_name);
    let mut i = 2;
    while candidate.exists() {
        candidate = root.join(format!("{safe_name}-{i}"));
        i += 1;
    }
    candidate
}

/// Resolves relative user paths from the current process directory.
fn absolute_from_current(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

/// Returns true when a tmux session already exists.
fn tmux_has_session(session: &str) -> Result<bool> {
    let status = ProcessCommand::new("tmux")
        .args(["has-session", "-t", session])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Runs a tmux command and converts failures to user-facing errors.
fn tmux(args: &[&str]) -> Result<()> {
    tmux_output(args).map(|_| ())
}

/// Runs a tmux command and returns stdout for commands that print pane ids.
fn tmux_output(args: &[&str]) -> Result<String> {
    let output = ProcessCommand::new("tmux").args(args).output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(GatError::Io(format!(
        "tmux {} failed\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

/// Extracts the pane id printed by `tmux -P -F '#{pane_id}'` commands.
fn tmux_pane_id<'a>(output: &'a str, command: &str) -> Result<&'a str> {
    let pane = output.trim();
    if pane.is_empty() {
        return Err(GatError::Io(format!(
            "tmux {command} did not return a pane id"
        )));
    }
    Ok(pane)
}

/// Checks whether a command is available on `PATH`.
fn command_exists(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(command).is_file())
}

/// Prints a human-facing progress line to stderr.
///
/// Progress goes to stderr so it never contaminates stdout, which scripts and
/// the shell integration parse. Setting `GAT_QUIET` suppresses these messages.
/// Lines are prefixed with `gat:` so they are easy to recognize and filter.
fn progress(message: &str) {
    if env::var_os("GAT_QUIET").is_some() {
        return;
    }
    eprintln!("gat: {message}");
}

/// Formats the stable tab-separated search feed.
///
/// Columns are `branch`, `state`, and `path`. This is intentionally simple so
/// users can pipe it to `fzf`, `awk`, or shell scripts without JSON tooling.
fn format_search_feed(listed: &[ListedWorktree]) -> String {
    let mut out = String::new();
    for item in listed {
        let wt = &item.worktree;
        let branch = wt
            .branch
            .as_deref()
            .unwrap_or_else(|| wt.head.as_deref().unwrap_or("<detached>"));
        let mut state = Vec::new();
        if wt.is_primary {
            state.push("primary");
        }
        if wt.detached {
            state.push("detached");
        }
        if item.dirty {
            state.push("dirty");
        }
        if item.merged {
            state.push("merged");
        }
        if wt.locked.is_some() {
            state.push("locked");
        }
        if wt.prunable.is_some() {
            state.push("missing");
        }
        let state = if state.is_empty() {
            "clean".to_string()
        } else {
            state.join(",")
        };
        out.push_str(&format!("{branch}\t{state}\t{}\n", path_string(&wt.path)));
    }
    out
}

/// Runs `fzf` with the worktree feed and returns the selected line.
fn run_fzf(feed: &str, query: Option<&str>) -> Result<String> {
    log::debug!("Starting fzf with {} bytes of feed data", feed.len());

    let mut command = ProcessCommand::new("fzf");
    command
        .arg("--delimiter")
        .arg("\t")
        .arg("--with-nth")
        .arg("1,2,3");
    if let Some(query) = query {
        command.arg("--query").arg(query);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // CRITICAL FIX: Write to stdin and drop it before waiting to avoid deadlock
    // If stdin buffer fills up while fzf is writing to stdout, we deadlock
    {
        if let Some(mut stdin) = child.stdin.take() {
            use io::Write;
            if let Err(e) = stdin.write_all(feed.as_bytes()) {
                log::warn!("Failed to write to fzf stdin: {e}");
            }
        } // stdin is dropped here, closing the pipe
    }

    let output = child.wait_with_output()?;
    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        log::debug!("fzf selected: {}", result);
        Ok(result)
    } else {
        log::debug!("fzf cancelled or failed");
        Ok(String::new())
    }
}

/// Extracts branch and path from a search feed selection.
fn parse_search_selection(selection: &str) -> Option<(String, String)> {
    let mut parts = selection.split('\t');
    let branch = parts.next()?.to_string();
    let _state = parts.next()?;
    let path = parts.next()?.to_string();
    Some((branch, path))
}

/// Emits shell functions that can `cd` after Rust computes a target path.
///
/// A child process cannot change the parent shell directory directly, so the
/// wrapper evaluates structured shell output from selected commands.
fn shell_init(args: ShellInitArgs) -> Result<String> {
    match args.shell {
        Shell::Bash | Shell::Zsh => Ok(r#"# gat shell integration
# Add this to your shell rc:
#   eval "$(gat shell-init --shell bash)"
gat() {
    local _gat_arg
    for _gat_arg in "$@"; do
        case "$_gat_arg" in
            --format|--format=*|--json|--shell)
                GAT_SHELL_INTEGRATION=1 command gat "$@"
                return
                ;;
        esac
    done

    if [ "$#" -gt 0 ]; then
        case "$1" in
            new|add|go|search|find)
                if [ "$1" = "search" ] || [ "$1" = "find" ]; then
                    local _gat_search_arg
                    for _gat_search_arg in "$@"; do
                        case "$_gat_search_arg" in
                            --print|--no-fzf|--path|--tmux)
                                GAT_SHELL_INTEGRATION=1 command gat "$@"
                                return
                                ;;
                        esac
                    done
                fi
                local _gat_out _gat_status
                unset GAT_STATUS GAT_ACTION GAT_TICKET GAT_BRANCH GAT_BASE GAT_TARGET GAT_PATH GAT_MESSAGE
                _gat_out="$(GAT_SHELL_INTEGRATION=1 command gat "$@" --format shell)"
                _gat_status=$?
                if [ $_gat_status -ne 0 ]; then
                    printf '%s\n' "$_gat_out" >&2
                    return $_gat_status
                fi
                eval "$_gat_out"
                if [ -n "${GAT_MESSAGE:-}" ]; then
                    printf '%s\n' "$GAT_MESSAGE"
                fi
                if [ -n "${GAT_PATH:-}" ] && [ -d "$GAT_PATH" ]; then
                    cd "$GAT_PATH" || return
                fi
                return 0
                ;;
            -*|help|version|list|ls|watch|path|switch|describe|desc|sessions|tmux|session|start|dx|docker|rm|remove|delete|archive|prune|shell-init|doctor)
                GAT_SHELL_INTEGRATION=1 command gat "$@"
                return
                ;;
            *)
                local _gat_out _gat_status
                unset GAT_STATUS GAT_ACTION GAT_TICKET GAT_BRANCH GAT_BASE GAT_TARGET GAT_PATH GAT_MESSAGE
                _gat_out="$(GAT_SHELL_INTEGRATION=1 command gat "$@" --format shell)"
                _gat_status=$?
                if [ $_gat_status -ne 0 ]; then
                    printf '%s\n' "$_gat_out" >&2
                    return $_gat_status
                fi
                eval "$_gat_out"
                if [ -n "${GAT_MESSAGE:-}" ]; then
                    printf '%s\n' "$GAT_MESSAGE"
                fi
                if [ -n "${GAT_PATH:-}" ] && [ -d "$GAT_PATH" ]; then
                    cd "$GAT_PATH" || return
                fi
                return 0
                ;;
        esac
    fi
    GAT_SHELL_INTEGRATION=1 command gat "$@"
}
"#
        .to_string()),
        Shell::Fish => Ok(r#"# gat fish shell integration
function gat
    command gat $argv
end
"#
        .to_string()),
    }
}

/// Builds a doctor hint about stale worktrees and prunes orphaned metadata.
///
/// Returns an empty string when there is nothing to report. As a side effect,
/// it drops metadata entries for worktrees Git no longer tracks so the store
/// does not grow without bound. The default staleness window is 30 days.
fn stale_worktree_hint(repo: &Repo) -> String {
    const STALE_DAYS: u64 = 30;

    let Ok(worktrees) = git::list_worktrees(Some(&repo.current_root)) else {
        return String::new();
    };
    let live_paths: std::collections::HashSet<String> =
        worktrees.iter().map(|wt| path_string(&wt.path)).collect();

    // Housekeeping: remove metadata for worktrees that no longer exist.
    let _ = MetadataStore::update(&repo.current_root, |m| {
        m.prune_missing(&live_paths);
    });

    let metadata = MetadataStore::load(&repo.current_root).unwrap_or_default();
    let mut stale: Vec<(String, u64)> = metadata
        .stale_worktrees(STALE_DAYS)
        .iter()
        .filter(|m| live_paths.contains(&m.path))
        .map(|m| {
            let days = metadata.days_since_access(&m.path).unwrap_or(0);
            (m.branch.clone(), days)
        })
        .collect();

    if stale.is_empty() {
        return String::new();
    }

    // Most idle first for a predictable, useful ordering.
    stale.sort_by(|a, b| b.1.cmp(&a.1));

    let mut hint = format!(
        "\nStale worktrees (>{STALE_DAYS}d unused): {}\n",
        stale.len()
    );
    for (branch, days) in &stale {
        hint.push_str(&format!("  {branch} - {days}d unused\n"));
    }
    hint.push_str(&format!("Run: gat prune --older-than {STALE_DAYS}\n"));
    hint
}

/// Reports environment and tool readiness for the workflow.
///
/// The command checks `tmux`, `codex`, `nvim`, and `fzf` because they are used
/// by higher-level session and search workflows.
fn doctor(args: DoctorArgs) -> Result<String> {
    let git_version = git::run_git(None, &["--version"])
        .map(|output| output.stdout.trim().to_string())
        .unwrap_or_else(|_| "git not found".to_string());
    let repo = git::discover_repo();
    let has_docker = command_exists("docker");
    let has_tmux = command_exists("tmux");
    let has_codex = command_exists("codex");
    let has_nvim = command_exists("nvim");
    let has_fzf = command_exists("fzf");
    let shell_integration = env::var_os("GAT_SHELL_INTEGRATION").is_some();

    match args.format {
        OutputFormat::Json => match repo {
            Ok(repo) => {
                let ticket_prefix = git::config_get(&repo, "gat.ticketPrefix")?;
                Ok(format!(
                    "{{\"status\":\"ok\",\"git\":\"{}\",\"current_root\":\"{}\",\"primary_root\":\"{}\",\"repo_name\":\"{}\",\"ticket_prefix\":{},\"shell_integration\":{},\"tools\":{{\"docker\":{},\"tmux\":{},\"codex\":{},\"nvim\":{},\"fzf\":{}}}}}\n",
                    json_escape(&git_version),
                    json_escape(&path_string(&repo.current_root)),
                    json_escape(&path_string(&repo.primary_root)),
                    json_escape(&repo.repo_name),
                    option_json(ticket_prefix.as_deref()),
                    shell_integration,
                    has_docker,
                    has_tmux,
                    has_codex,
                    has_nvim,
                    has_fzf
                ))
            }
            Err(err) => Ok(format!(
                "{{\"status\":\"error\",\"git\":\"{}\",\"message\":\"{}\",\"shell_integration\":{},\"tools\":{{\"docker\":{},\"tmux\":{},\"codex\":{},\"nvim\":{},\"fzf\":{}}}}}\n",
                json_escape(&git_version),
                json_escape(&err.to_string()),
                shell_integration,
                has_docker,
                has_tmux,
                has_codex,
                has_nvim,
                has_fzf
            )),
        },
        OutputFormat::Text | OutputFormat::Shell => match repo {
            Ok(repo) => {
                let ticket_prefix =
                    git::config_get(&repo, "gat.ticketPrefix")?.unwrap_or_else(|| "TICKET".into());
                let shell_hint = if shell_integration {
                    ""
                } else {
                    "\nShell integration is inactive. Add `eval \"$(gat shell-init --shell bash)\"` to enable automatic `cd` for `gat 12345`, `gat go`, and `gat search`.\n"
                };
                let stale_hint = stale_worktree_hint(&repo);
                Ok(format!(
                    "gat doctor\nGit: {git_version}\nCurrent root: {}\nPrimary root: {}\nRepo name: {}\nRepo parent: {}\nTicket prefix: {}\nShell integration: {}\nTools: docker={} tmux={} codex={} nvim={} fzf={}{stale_hint}{shell_hint}",
                    path_string(&repo.current_root),
                    path_string(&repo.primary_root),
                    repo.repo_name,
                    path_string(&repo.repo_parent),
                    ticket_prefix,
                    yes_no(shell_integration),
                    yes_no(has_docker),
                    yes_no(has_tmux),
                    yes_no(has_codex),
                    yes_no(has_nvim),
                    yes_no(has_fzf)
                ))
            }
            Err(err) => Ok(format!(
                "gat doctor\nGit: {git_version}\nRepository: {err}\nShell integration: {}\nTools: docker={} tmux={} codex={} nvim={} fzf={}\n",
                yes_no(shell_integration),
                yes_no(has_docker),
                yes_no(has_tmux),
                yes_no(has_codex),
                yes_no(has_nvim),
                yes_no(has_fzf)
            )),
        },
    }
}

/// Computes the worktree creation/reuse plan for a target.
///
/// Keeping this as a pure planning step makes `--dry-run`, `gat tmux`, and
/// `gat new` consistent.
fn build_new_plan(repo: &Repo, worktrees: &[Worktree], args: &NewArgs) -> Result<NewPlan> {
    let ticket = normalize_target(repo, &args.target, args.prefix.as_deref(), args.no_prefix)?;
    let branch = args.branch.clone().unwrap_or_else(|| ticket.clone());
    if let Some(existing) = git::find_worktree(worktrees, &branch) {
        return Ok(NewPlan {
            ticket,
            branch,
            base: existing.head.clone().unwrap_or_else(|| "HEAD".to_string()),
            path: existing.path.clone(),
            action: if args.dry_run {
                NewAction::DryRun
            } else {
                NewAction::Existing
            },
            detach: existing.detached,
        });
    }

    let base = match &args.base {
        Some(base) => resolve_base_shortcut(repo, worktrees, base)?,
        None => git::default_branch(repo, worktrees)?,
    };
    let path = args
        .path
        .clone()
        .unwrap_or_else(|| default_worktree_path(repo, &ticket));

    if path.exists() && !args.dry_run {
        return Err(GatError::Unsafe(format!(
            "path already exists and is not registered as a worktree: {}",
            path_string(&path)
        )));
    }

    Ok(NewPlan {
        ticket,
        branch,
        base,
        path,
        action: if args.dry_run {
            NewAction::DryRun
        } else {
            NewAction::Create
        },
        detach: args.detach,
    })
}

/// Normalizes user ticket input into the branch/worktree ticket name.
///
/// Plain numeric targets get a prefix, defaulting to `TICKET`. Already-prefixed or
/// non-numeric targets are preserved.
fn normalize_target(
    repo: &Repo,
    raw: &str,
    prefix_override: Option<&str>,
    no_prefix: bool,
) -> Result<String> {
    if no_prefix || !is_plain_number(raw) {
        return Ok(raw.to_string());
    }

    let prefix = match prefix_override {
        Some(prefix) => Some(prefix.to_string()),
        None => git::config_get(repo, "gat.ticketPrefix")?,
    }
    .unwrap_or_else(|| "TICKET".to_string());

    let prefix = normalize_prefix(&prefix)?;
    Ok(format!("{prefix}-{raw}"))
}

/// Returns true when a target is only ASCII digits.
fn is_plain_number(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

/// Validates and normalizes a ticket prefix.
fn normalize_prefix(value: &str) -> Result<String> {
    let prefix = value.trim().trim_end_matches('-');
    if prefix.is_empty() {
        return Err(GatError::Usage(
            "--prefix requires at least one non-hyphen character".into(),
        ));
    }
    if !prefix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
    {
        return Err(GatError::Usage(format!(
            "invalid prefix {value}; use letters, digits, dots, underscores, or hyphens"
        )));
    }
    Ok(prefix.to_string())
}

/// Computes the default sibling worktree path.
///
/// Example: `/repos/data-processing` + `TICKET-12345` becomes
/// `/repos/data-processing-TICKET-12345`.
fn default_worktree_path(repo: &Repo, ticket: &str) -> PathBuf {
    repo.repo_parent.join(format!(
        "{}-{}",
        repo.repo_name,
        sanitize_path_component(ticket)
    ))
}

/// Converts user text into a filesystem-safe path component.
///
/// Git branch names may contain slashes or other characters that are awkward in
/// a sibling directory name. The branch remains unchanged; only the path segment
/// is sanitized.
fn sanitize_path_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_dash = false;
    for ch in value.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-';
        if keep {
            out.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "worktree".to_string()
    } else {
        trimmed
    }
}

/// Maximum number of description characters folded into a tmux session name.
const SESSION_DESC_MAX: usize = 100;

/// Returns the stable session-name prefix for a ticket: `gat-<ticket>`.
///
/// This prefix is used both as the base of the full session name and to find an
/// existing session for a ticket whose description has since changed.
fn session_prefix(ticket: &str) -> String {
    format!("gat-{}", sanitize_path_component(ticket))
}

/// Builds a tmux session name from a ticket and optional description.
///
/// tmux treats `.` and `:` specially in target names and dislikes spaces, so the
/// description is sanitized and truncated to [`SESSION_DESC_MAX`] characters and
/// appended to the `gat-<ticket>` prefix. With no description the name is just
/// the prefix, preserving the previous behavior.
fn session_name(ticket: &str, description: Option<&str>) -> String {
    let prefix = session_prefix(ticket);
    match description {
        Some(desc) if !desc.trim().is_empty() => {
            let truncated: String = desc.trim().chars().take(SESSION_DESC_MAX).collect();
            let suffix = sanitize_path_component(&truncated);
            if suffix.is_empty() || suffix == "worktree" {
                prefix
            } else {
                format!("{prefix}-{suffix}")
            }
        }
        _ => prefix,
    }
}

/// Finds an existing gat session for a ticket, tolerating description changes.
///
/// Returns the first live session whose name is exactly `gat-<ticket>` or begins
/// with `gat-<ticket>-`. This lets `switch`/`tmux` reattach even if the stored
/// description (and thus the computed name) changed since the session started.
fn find_existing_session(ticket: &str) -> Result<Option<String>> {
    if !command_exists("tmux") {
        return Ok(None);
    }
    let prefix = session_prefix(ticket);
    let output = ProcessCommand::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()?;
    if !output.status.success() {
        // No server running or no sessions: nothing to match.
        return Ok(None);
    }
    let with_dash = format!("{prefix}-");
    let found = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|name| *name == prefix || name.starts_with(&with_dash))
        .map(ToOwned::to_owned);
    Ok(found)
}

/// Stores worktree metadata on the tmux session for tooling and at-a-glance info.
///
/// Sets user options `@gat_path`, `@gat_branch`, and `@gat_description` on the
/// session. Failures are non-fatal: the session is still usable without them.
fn set_session_metadata(session: &str, path: &Path, branch: &str, description: Option<&str>) {
    let path = path_string(path);
    let _ = tmux(&["set-option", "-t", session, "@gat_path", &path]);
    let _ = tmux(&["set-option", "-t", session, "@gat_branch", branch]);
    if let Some(desc) = description.filter(|d| !d.trim().is_empty()) {
        let _ = tmux(&["set-option", "-t", session, "@gat_description", desc]);
    }
}

/// Resolves base-ref shortcuts used by `--base` and `--from`.
fn resolve_base_shortcut(repo: &Repo, worktrees: &[Worktree], value: &str) -> Result<String> {
    match value {
        "^" => git::default_branch(repo, worktrees),
        "@" => git::current_branch(repo)?.ok_or_else(|| {
            GatError::Unsafe("current worktree is detached; cannot resolve @".into())
        }),
        _ => Ok(value.to_string()),
    }
}

/// Resolves navigation shortcuts such as `@` and `^`.
///
/// `-` is reserved for future previous-worktree state and currently returns
/// `None`.
fn resolve_shortcut_path(
    repo: &Repo,
    worktrees: &[Worktree],
    target: &str,
) -> Result<Option<PathBuf>> {
    match target {
        "^" => {
            let default = git::default_branch(repo, worktrees)?;
            Ok(git::find_worktree(worktrees, &default).map(|wt| wt.path.clone()))
        }
        "@" => Ok(Some(repo.current_root.clone())),
        "-" => Ok(None),
        _ => Ok(None),
    }
}

/// Ensures the parent directory for a path exists.
fn ensure_parent_exists(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Formats create/reuse/dry-run output.
fn format_new_result(plan: &NewPlan, format: OutputFormat) -> Result<String> {
    let action = match plan.action {
        NewAction::Create => "created",
        NewAction::Existing => "existing",
        NewAction::DryRun => "would_create",
    };
    match format {
        OutputFormat::Text => Ok(format!(
            "{action} worktree for {} on branch {} from {} @ {}\n",
            plan.ticket,
            plan.branch,
            plan.base,
            path_string(&plan.path)
        )),
        OutputFormat::Json => Ok(format!(
            "{{\"status\":\"ok\",\"action\":\"{}\",\"ticket\":\"{}\",\"branch\":\"{}\",\"base\":\"{}\",\"path\":\"{}\",\"detach\":{}}}\n",
            action,
            json_escape(&plan.ticket),
            json_escape(&plan.branch),
            json_escape(&plan.base),
            json_escape(&path_string(&plan.path)),
            plan.detach
        )),
        OutputFormat::Shell => Ok(format!(
            "GAT_STATUS=ok\nGAT_ACTION={}\nGAT_TICKET={}\nGAT_BRANCH={}\nGAT_BASE={}\nGAT_PATH={}\nGAT_MESSAGE={}\n",
            shell_escape(action),
            shell_escape(&plan.ticket),
            shell_escape(&plan.branch),
            shell_escape(&plan.base),
            shell_escape(&path_string(&plan.path)),
            shell_escape(&format!(
                "{action} worktree for {} @ {}",
                plan.ticket,
                path_string(&plan.path)
            ))
        )),
    }
}

/// Formats path-switch output.
///
/// Shell mode emits `GAT_PATH` so the shell wrapper can perform `cd`.
fn format_path_result(
    action: &str,
    target: &str,
    path: &Path,
    format: OutputFormat,
) -> Result<String> {
    match format {
        OutputFormat::Text => Ok(format!("{} @ {}\n", target, path_string(path))),
        OutputFormat::Json => Ok(format!(
            "{{\"status\":\"ok\",\"action\":\"{}\",\"target\":\"{}\",\"path\":\"{}\"}}\n",
            json_escape(action),
            json_escape(target),
            json_escape(&path_string(path))
        )),
        OutputFormat::Shell => Ok(format!(
            "GAT_STATUS=ok\nGAT_ACTION={}\nGAT_TARGET={}\nGAT_PATH={}\nGAT_MESSAGE={}\n",
            shell_escape(action),
            shell_escape(target),
            shell_escape(&path_string(path)),
            shell_escape(&format!("switching to {} @ {}", target, path_string(path)))
        )),
    }
}

/// Formats the human-readable worktree table.
fn format_list_text(repo: &Repo, listed: &[ListedWorktree]) -> String {
    let mut out = String::from(
        "Branch                         State      HEAD      Changes      Idle   Path\n",
    );
    for item in listed {
        let wt = &item.worktree;
        let marker = if wt.path == repo.current_root {
            "@"
        } else {
            " "
        };
        let branch = wt
            .branch
            .clone()
            .unwrap_or_else(|| format!("detached:{}", git::short_head(wt.head.as_deref())));
        let mut states = Vec::new();
        if wt.is_primary {
            states.push("primary");
        }
        if wt.detached {
            states.push("detached");
        }
        if item.dirty {
            states.push("dirty");
        }
        if item.merged {
            states.push("merged");
        }
        if wt.prunable.is_some() {
            states.push("missing");
        }
        if wt.locked.is_some() {
            states.push("locked");
        }
        let state = if states.is_empty() {
            "clean".to_string()
        } else {
            states.join(",")
        };
        let changes = format_changes(item);
        let idle = format_idle(item.idle_days);
        let description = item
            .description
            .as_deref()
            .map(|d| format!("  {d}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{marker} {:<30} {:<10} {:<8} {:<12} {:<6} {}{}\n",
            branch,
            state,
            git::short_head(wt.head.as_deref()),
            changes,
            idle,
            path_string(&wt.path),
            description
        ));
    }
    out
}

/// Formats the working-tree change summary for the list table.
///
/// Shows `Nf +I -D` where `N` is changed files, `I` insertions, and `D`
/// deletions. A clean worktree renders as `-`.
fn format_changes(item: &ListedWorktree) -> String {
    if item.changed_files == 0 {
        return "-".to_string();
    }
    format!(
        "{}f +{} -{}",
        item.changed_files, item.insertions, item.deletions
    )
}

/// Formats idle-days for the list table, using a compact `Nd` form.
fn format_idle(idle_days: Option<u64>) -> String {
    match idle_days {
        Some(days) => format!("{days}d"),
        None => "-".to_string(),
    }
}

/// Formats worktree listing as JSON.
fn format_list_json(listed: &[ListedWorktree]) -> String {
    let entries = listed
        .iter()
        .map(|item| {
            let wt = &item.worktree;
            format!(
                "{{\"path\":\"{}\",\"branch\":{},\"head\":{},\"primary\":{},\"detached\":{},\"dirty\":{},\"merged\":{},\"changed_files\":{},\"insertions\":{},\"deletions\":{},\"locked\":{},\"prunable\":{},\"idle_days\":{},\"description\":{}}}",
                json_escape(&path_string(&wt.path)),
                option_json(wt.branch.as_deref()),
                option_json(wt.head.as_deref()),
                wt.is_primary,
                wt.detached,
                item.dirty,
                item.merged,
                item.changed_files,
                item.insertions,
                item.deletions,
                option_json(wt.locked.as_deref()),
                option_json(wt.prunable.as_deref()),
                item.idle_days.map(|d| d.to_string()).unwrap_or_else(|| "null".to_string()),
                option_json(item.description.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{entries}]\n")
}

/// Formats an optional string value for manually generated JSON.
fn option_json(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

/// Formats booleans for human-readable diagnostics.
fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// Formats remove/delete output.
fn format_remove_result(action: &str, wt: &Worktree, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Text | OutputFormat::Shell => Ok(format!(
            "{action} worktree {} @ {}\n",
            wt.branch.as_deref().unwrap_or("<detached>"),
            path_string(&wt.path)
        )),
        OutputFormat::Json => Ok(format!(
            "{{\"status\":\"ok\",\"action\":\"{}\",\"branch\":{},\"path\":\"{}\"}}\n",
            json_escape(action),
            option_json(wt.branch.as_deref()),
            json_escape(&path_string(&wt.path))
        )),
    }
}

/// Prompts for explicit confirmation on destructive operations.
fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

#[cfg(test)]
mod tests {
    use super::{
        format_sessions, is_plain_number, normalize_prefix, parse_session_line,
        sanitize_path_component, session_name, session_prefix, SESSION_DESC_MAX,
    };
    use crate::output::OutputFormat;

    #[test]
    fn sanitizes_path_component() {
        assert_eq!(sanitize_path_component("TICKET-12345"), "TICKET-12345");
        assert_eq!(
            sanitize_path_component("feature/auth login"),
            "feature-auth-login"
        );
        assert_eq!(sanitize_path_component("///"), "worktree");
    }

    #[test]
    fn detects_plain_numeric_ticket() {
        assert!(is_plain_number("12345"));
        assert!(!is_plain_number("TICKET-12345"));
        assert!(!is_plain_number("12345A"));
        assert!(!is_plain_number(""));
    }

    #[test]
    fn normalizes_prefix() {
        assert_eq!(normalize_prefix("TICKET").unwrap(), "TICKET");
        assert_eq!(normalize_prefix("TICKET-").unwrap(), "TICKET");
        assert!(normalize_prefix("").is_err());
        assert!(normalize_prefix("TICKET/BUG").is_err());
    }

    #[test]
    fn session_name_without_description_is_prefix() {
        assert_eq!(session_prefix("TICKET-123"), "gat-TICKET-123");
        assert_eq!(session_name("TICKET-123", None), "gat-TICKET-123");
        assert_eq!(session_name("TICKET-123", Some("   ")), "gat-TICKET-123");
    }

    #[test]
    fn session_name_folds_in_description() {
        assert_eq!(
            session_name("TICKET-123", Some("fix login redirect")),
            "gat-TICKET-123-fix-login-redirect"
        );
        // Characters tmux dislikes are sanitized away.
        assert_eq!(
            session_name("TICKET-123", Some("feature: a.b/c")),
            "gat-TICKET-123-feature-a.b-c"
        );
    }

    #[test]
    fn session_name_truncates_long_descriptions() {
        let long = "x".repeat(SESSION_DESC_MAX + 50);
        let name = session_name("TICKET-123", Some(&long));
        let suffix = name.strip_prefix("gat-TICKET-123-").unwrap();
        assert_eq!(suffix.len(), SESSION_DESC_MAX);
    }

    #[test]
    fn session_name_ignores_description_that_sanitizes_to_nothing() {
        // A description of only punctuation collapses to the bare prefix.
        assert_eq!(session_name("TICKET-123", Some("///")), "gat-TICKET-123");
    }

    #[test]
    fn parse_session_line_extracts_gat_fields() {
        let line = "gat-TICKET-123-fix\t1\t3\tTICKET-123\t/repo/wt\tfix login bug";
        let session = parse_session_line(line).expect("should parse gat session");
        assert_eq!(session.name, "gat-TICKET-123-fix");
        assert!(session.attached);
        assert_eq!(session.windows, 3);
        assert_eq!(session.branch.as_deref(), Some("TICKET-123"));
        assert_eq!(session.path.as_deref(), Some("/repo/wt"));
        assert_eq!(session.description.as_deref(), Some("fix login bug"));
    }

    #[test]
    fn parse_session_line_handles_missing_metadata() {
        // Detached session with no @gat_* options set.
        let line = "gat-TICKET-999\t0\t1\t\t\t";
        let session = parse_session_line(line).expect("should parse gat session");
        assert!(!session.attached);
        assert_eq!(session.windows, 1);
        assert_eq!(session.branch, None);
        assert_eq!(session.path, None);
        assert_eq!(session.description, None);
    }

    #[test]
    fn parse_session_line_skips_non_gat_sessions() {
        assert!(parse_session_line("work\t1\t2\t\t\t").is_none());
        assert!(parse_session_line("main-session\t0\t1\t\t\t").is_none());
    }

    #[test]
    fn format_sessions_empty_is_friendly() {
        assert_eq!(
            format_sessions(&[], OutputFormat::Text),
            "No gat tmux sessions.\n"
        );
        assert_eq!(format_sessions(&[], OutputFormat::Json), "[]\n");
    }

    #[test]
    fn format_sessions_json_includes_fields() {
        let session = parse_session_line("gat-TICKET-1\t1\t2\tTICKET-1\t/wt\tcache work").unwrap();
        let json = format_sessions(&[session], OutputFormat::Json);
        assert!(json.contains("\"name\":\"gat-TICKET-1\""));
        assert!(json.contains("\"attached\":true"));
        assert!(json.contains("\"windows\":2"));
        assert!(json.contains("\"branch\":\"TICKET-1\""));
        assert!(json.contains("\"description\":\"cache work\""));
    }
}
