//! Thin wrappers around Git commands.
//!
//! `gat` shells out to the installed `git` binary rather than using libgit2.
//! That keeps behavior aligned with the user's Git version, config, credential
//! helpers, and native worktree implementation.

use crate::error::{GatError, Result};
use crate::output::path_string;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Repository context discovered from the current process directory.
///
/// `current_root` may be a linked worktree. `primary_root` is the first entry
/// from `git worktree list`, which Git reports as the main worktree.
#[derive(Clone, Debug)]
pub struct Repo {
    /// Root of the worktree the command is currently running inside.
    pub current_root: PathBuf,
    /// Root of the primary worktree for the repository.
    pub primary_root: PathBuf,
    /// Basename of the primary worktree, used in default path templates.
    pub repo_name: String,
    /// Parent directory of the primary worktree.
    pub repo_parent: PathBuf,
}

/// Parsed entry from `git worktree list --porcelain -z`.
#[derive(Clone, Debug)]
pub struct Worktree {
    /// Absolute or Git-reported path to the worktree.
    pub path: PathBuf,
    /// Full HEAD object id when Git reports one.
    pub head: Option<String>,
    /// Local branch name without the `refs/heads/` prefix.
    pub branch: Option<String>,
    /// Whether Git reports this entry as bare.
    pub bare: bool,
    /// Whether the worktree is detached.
    pub detached: bool,
    /// Optional Git lock reason.
    pub locked: Option<String>,
    /// Optional Git prune reason.
    pub prunable: Option<String>,
    /// Whether this is the first worktree returned by Git.
    pub is_primary: bool,
}

/// Captured output from a Git subprocess.
#[derive(Debug)]
pub struct GitOutput {
    /// Standard output decoded lossily as UTF-8.
    pub stdout: String,
    /// Standard error decoded lossily as UTF-8.
    pub stderr: String,
    /// Process exit status, or `1` when no status code is available.
    pub status: i32,
}

/// Runs `git` with optional working directory and captures output.
///
/// This function intentionally does not treat non-zero exit as an error; callers
/// such as [`branch_exists`] and [`config_get`] need to inspect status codes.
pub fn run_git(cwd: Option<&Path>, args: &[&str]) -> Result<GitOutput> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().map_err(GatError::from)?;
    let status = output.status.code().unwrap_or(1);
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status,
    })
}

/// Runs `git` and converts non-zero exit status into [`GatError::Git`].
pub fn git_ok(cwd: Option<&Path>, args: &[&str]) -> Result<GitOutput> {
    let output = run_git(cwd, args)?;
    if output.status == 0 {
        return Ok(output);
    }
    Err(GatError::Git {
        command: format!("git {}", args.join(" ")),
        message: clean_git_message(&output),
    })
}

/// Discovers repository metadata from the current process directory.
///
/// The primary worktree drives default sibling paths. That means running `gat`
/// from a linked worktree still creates future worktrees next to the primary
/// checkout rather than next to the linked one.
pub fn discover_repo() -> Result<Repo> {
    let current_root_output = run_git(None, &["rev-parse", "--show-toplevel"])?;
    if current_root_output.status != 0 {
        return Err(GatError::NotGitRepo);
    }
    let current_root = PathBuf::from(current_root_output.stdout.trim());
    let worktrees = list_worktrees(Some(&current_root))?;
    let primary_root = worktrees
        .first()
        .map(|wt| wt.path.clone())
        .unwrap_or_else(|| current_root.clone());
    let repo_name = primary_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            GatError::Io(format!(
                "cannot derive repo name from {}",
                path_string(&primary_root)
            ))
        })?
        .to_string();
    let repo_parent = primary_root
        .parent()
        .ok_or_else(|| {
            GatError::Io(format!(
                "cannot derive repo parent from {}",
                path_string(&primary_root)
            ))
        })?
        .to_path_buf();

    Ok(Repo {
        current_root,
        primary_root,
        repo_name,
        repo_parent,
    })
}

/// Lists all registered Git worktrees using the stable porcelain format.
pub fn list_worktrees(cwd: Option<&Path>) -> Result<Vec<Worktree>> {
    let output = git_ok(cwd, &["worktree", "list", "--porcelain", "-z"])?;
    Ok(parse_worktrees(&output.stdout))
}

/// Parses NUL-delimited `git worktree list --porcelain -z` output.
fn parse_worktrees(raw: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<Worktree> = None;

    for token in raw.split('\0') {
        if token.is_empty() {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            continue;
        }

        if let Some(path) = token.strip_prefix("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            current = Some(Worktree {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                locked: None,
                prunable: None,
                is_primary: worktrees.is_empty(),
            });
            continue;
        }

        let Some(wt) = current.as_mut() else {
            continue;
        };

        if let Some(head) = token.strip_prefix("HEAD ") {
            wt.head = Some(head.to_string());
        } else if let Some(branch) = token.strip_prefix("branch ") {
            wt.branch = Some(branch.trim_start_matches("refs/heads/").to_string());
        } else if token == "bare" {
            wt.bare = true;
        } else if token == "detached" {
            wt.detached = true;
        } else if let Some(reason) = token.strip_prefix("locked ") {
            wt.locked = Some(reason.to_string());
        } else if token == "locked" {
            wt.locked = Some(String::new());
        } else if let Some(reason) = token.strip_prefix("prunable ") {
            wt.prunable = Some(reason.to_string());
        } else if token == "prunable" {
            wt.prunable = Some(String::new());
        }
    }

    if let Some(wt) = current {
        worktrees.push(wt);
    }

    for (index, wt) in worktrees.iter_mut().enumerate() {
        wt.is_primary = index == 0;
    }

    worktrees
}

/// Returns true when a local branch exists.
pub fn branch_exists(repo: &Repo, branch: &str) -> Result<bool> {
    let output = run_git(
        Some(&repo.current_root),
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )?;
    Ok(output.status == 0)
}

/// Returns the current branch, or `None` for detached HEAD or Git failure.
pub fn current_branch(repo: &Repo) -> Result<Option<String>> {
    let output = run_git(Some(&repo.current_root), &["branch", "--show-current"])?;
    if output.status != 0 {
        return Ok(None);
    }
    let branch = output.stdout.trim();
    if branch.is_empty() {
        Ok(None)
    } else {
        Ok(Some(branch.to_string()))
    }
}

/// Reads a Git config value from the repository context.
///
/// This intentionally uses `git config --get`, so local, global, and included
/// Git config files are resolved exactly as Git would resolve them.
pub fn config_get(repo: &Repo, key: &str) -> Result<Option<String>> {
    let output = run_git(Some(&repo.current_root), &["config", "--get", key])?;
    if output.status != 0 {
        return Ok(None);
    }
    let value = output.stdout.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

/// Resolves the default base branch for new ticket worktrees.
///
/// Resolution order favors the remote default branch when available, then common
/// local defaults, then the primary worktree branch, and finally `HEAD`.
pub fn default_branch(repo: &Repo, worktrees: &[Worktree]) -> Result<String> {
    let origin_head = run_git(
        Some(&repo.current_root),
        &["symbolic-ref", "refs/remotes/origin/HEAD"],
    )?;
    if origin_head.status == 0 {
        let value = origin_head.stdout.trim();
        if let Some(branch) = value.strip_prefix("refs/remotes/origin/") {
            return Ok(branch.to_string());
        }
    }

    for candidate in ["main", "master"] {
        if branch_exists(repo, candidate)? {
            return Ok(candidate.to_string());
        }
    }

    if let Some(branch) = worktrees.first().and_then(|wt| wt.branch.clone()) {
        return Ok(branch);
    }

    Ok("HEAD".to_string())
}

/// Finds a worktree by exact branch, basename, or full path.
pub fn find_worktree<'a>(worktrees: &'a [Worktree], target: &str) -> Option<&'a Worktree> {
    worktrees.iter().find(|wt| {
        wt.branch.as_deref() == Some(target)
            || wt.path.file_name().and_then(|name| name.to_str()) == Some(target)
            || path_string(&wt.path) == target
    })
}

/// Adds a worktree, creating the branch when it does not already exist.
///
/// The function avoids `git worktree add -B` deliberately; resetting an existing
/// branch is too destructive for a default ticket workflow.
pub fn add_worktree(
    repo: &Repo,
    path: &Path,
    branch: &str,
    base: &str,
    detach: bool,
) -> Result<()> {
    if detach {
        return git_ok(
            Some(&repo.current_root),
            &["worktree", "add", "--detach", path_arg(path).as_str(), base],
        )
        .map(|_| ());
    }

    if branch_exists(repo, branch)? {
        git_ok(
            Some(&repo.current_root),
            &["worktree", "add", path_arg(path).as_str(), branch],
        )
        .map(|_| ())
    } else {
        git_ok(
            Some(&repo.current_root),
            &[
                "worktree",
                "add",
                "-b",
                branch,
                path_arg(path).as_str(),
                base,
            ],
        )
        .map(|_| ())
    }
}

/// Removes a worktree through native Git.
pub fn remove_worktree(repo: &Repo, path: &Path, force: bool) -> Result<()> {
    let path = path_arg(path);
    if force {
        git_ok(
            Some(&repo.current_root),
            &["worktree", "remove", "--force", path.as_str()],
        )
    } else {
        git_ok(
            Some(&repo.current_root),
            &["worktree", "remove", path.as_str()],
        )
    }
    .map(|_| ())
}

/// Moves a registered worktree through native Git.
///
/// This is used by archive flows so Git metadata stays consistent.
pub fn move_worktree(repo: &Repo, from: &Path, to: &Path) -> Result<()> {
    let from = path_arg(from);
    let to = path_arg(to);
    git_ok(
        Some(&repo.current_root),
        &["worktree", "move", from.as_str(), to.as_str()],
    )
    .map(|_| ())
}

/// Prunes stale Git worktree metadata and returns Git's textual output.
pub fn prune_stale(repo: &Repo, dry_run: bool) -> Result<String> {
    let args = if dry_run {
        vec![
            "worktree",
            "prune",
            "--verbose",
            "--dry-run",
            "--expire=now",
        ]
    } else {
        vec!["worktree", "prune", "--verbose", "--expire=now"]
    };
    let output = git_ok(Some(&repo.current_root), &args)?;
    Ok(if output.stderr.trim().is_empty() {
        output.stdout
    } else {
        output.stderr
    })
}

/// Working-tree change summary for a single worktree.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkingStatus {
    /// Whether there are any staged, unstaged, or untracked changes.
    pub dirty: bool,
    /// Number of changed paths reported by `git status --porcelain`.
    ///
    /// This counts staged, unstaged, and untracked entries, matching the
    /// "number of files touched" a user sees in their working tree.
    pub changed_files: usize,
}

/// Line-level diff summary versus `HEAD`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiffStat {
    /// Inserted lines across tracked files versus `HEAD`.
    pub insertions: usize,
    /// Deleted lines across tracked files versus `HEAD`.
    pub deletions: usize,
}

/// Returns the working-tree change summary for a worktree.
///
/// A single `git status --porcelain` call provides both the dirty flag and the
/// changed-file count, so callers that need either avoid extra subprocesses.
pub fn working_status(path: &Path) -> Result<WorkingStatus> {
    if !path.exists() {
        return Ok(WorkingStatus::default());
    }
    let output = git_ok(Some(path), &["status", "--porcelain"])?;
    let changed_files = output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Ok(WorkingStatus {
        dirty: changed_files > 0,
        changed_files,
    })
}

/// Returns true when the worktree has staged, unstaged, or untracked changes.
pub fn is_dirty(path: &Path) -> Result<bool> {
    Ok(working_status(path)?.dirty)
}

/// Returns the insertion/deletion line counts for tracked changes versus `HEAD`.
///
/// Untracked files are not included, mirroring `git diff` semantics. Returns a
/// zeroed stat when the worktree is missing or has no tracked changes.
pub fn diff_stat(path: &Path) -> Result<DiffStat> {
    if !path.exists() {
        return Ok(DiffStat::default());
    }
    let output = git_ok(Some(path), &["diff", "--shortstat", "HEAD"])?;
    Ok(parse_shortstat(&output.stdout))
}

/// Parses `git diff --shortstat` output into a [`DiffStat`].
///
/// Example input: ` 3 files changed, 12 insertions(+), 4 deletions(-)`.
fn parse_shortstat(raw: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for segment in raw.split(',') {
        let segment = segment.trim();
        let Some((count, label)) = segment.split_once(' ') else {
            continue;
        };
        let Ok(value) = count.parse::<usize>() else {
            continue;
        };
        if label.starts_with("insertion") {
            stat.insertions = value;
        } else if label.starts_with("deletion") {
            stat.deletions = value;
        }
    }
    stat
}

/// Returns true when `branch` is an ancestor of `default_branch`.
pub fn is_merged(repo: &Repo, branch: &str, default_branch: &str) -> Result<bool> {
    if branch == default_branch || branch == "HEAD" {
        return Ok(false);
    }
    let output = run_git(
        Some(&repo.current_root),
        &["merge-base", "--is-ancestor", branch, default_branch],
    )?;
    Ok(output.status == 0)
}

/// Deletes a local branch using `git branch -d` or `-D`.
pub fn delete_branch(repo: &Repo, branch: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    git_ok(Some(&repo.current_root), &["branch", flag, branch]).map(|_| ())
}

/// Returns the branch currently checked out in the primary worktree, if any.
pub fn branch_at(path: &Path) -> Result<Option<String>> {
    let output = run_git(Some(path), &["branch", "--show-current"])?;
    if output.status != 0 {
        return Ok(None);
    }
    let branch = output.stdout.trim();
    if branch.is_empty() {
        Ok(None)
    } else {
        Ok(Some(branch.to_string()))
    }
}

/// Merges `branch` into `into_branch`, running the merge in `workdir`.
///
/// `workdir` must have `into_branch` checked out (typically the primary
/// worktree). When `no_ff` is set, a merge commit is always created. Returns the
/// merge command's combined output for display.
pub fn merge_branch(
    workdir: &Path,
    branch: &str,
    into_branch: &str,
    no_ff: bool,
) -> Result<String> {
    let message = format!("Merge branch '{branch}' into {into_branch}");
    let mut args = vec!["merge", "--no-edit"];
    if no_ff {
        args.push("--no-ff");
        args.push("-m");
        args.push(&message);
    }
    args.push(branch);
    let output = git_ok(Some(workdir), &args)?;
    Ok(if output.stdout.trim().is_empty() {
        output.stderr
    } else {
        output.stdout
    })
}

/// Aborts an in-progress merge in `workdir` (best effort).
pub fn merge_abort(workdir: &Path) -> Result<()> {
    // Ignore failure: there may be no merge in progress.
    let _ = run_git(Some(workdir), &["merge", "--abort"]);
    Ok(())
}

/// Shortens a HEAD object id for status displays.
pub fn short_head(head: Option<&str>) -> String {
    head.map(|value| value.chars().take(8).collect())
        .unwrap_or_else(|| "????????".to_string())
}

/// Chooses the most useful Git error message for users.
fn clean_git_message(output: &GitOutput) -> String {
    let stderr = output.stderr.trim();
    let stdout = output.stdout.trim();
    if !stderr.is_empty() {
        stderr.to_string()
    } else if !stdout.is_empty() {
        stdout.to_string()
    } else {
        format!("exit status {}", output.status)
    }
}

/// Converts a path to an owned string suitable for process arguments.
fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{parse_shortstat, parse_worktrees};

    #[test]
    fn parses_porcelain_z_worktrees() {
        let raw = concat!(
            "worktree /repo\0",
            "HEAD abcdef123456\0",
            "branch refs/heads/master\0",
            "\0",
            "worktree /repo-feature\0",
            "HEAD 123456abcdef\0",
            "branch refs/heads/feature/test\0",
            "\0"
        );

        let worktrees = parse_worktrees(raw);

        assert_eq!(worktrees.len(), 2);
        assert!(worktrees[0].is_primary);
        assert_eq!(worktrees[0].branch.as_deref(), Some("master"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("feature/test"));
    }

    #[test]
    fn parses_shortstat_variants() {
        let full = parse_shortstat(" 3 files changed, 12 insertions(+), 4 deletions(-)");
        assert_eq!(full.insertions, 12);
        assert_eq!(full.deletions, 4);

        let only_insertions = parse_shortstat(" 1 file changed, 5 insertions(+)");
        assert_eq!(only_insertions.insertions, 5);
        assert_eq!(only_insertions.deletions, 0);

        let only_deletions = parse_shortstat(" 2 files changed, 7 deletions(-)");
        assert_eq!(only_deletions.insertions, 0);
        assert_eq!(only_deletions.deletions, 7);

        let empty = parse_shortstat("");
        assert_eq!(empty.insertions, 0);
        assert_eq!(empty.deletions, 0);
    }
}
