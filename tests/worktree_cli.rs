//! CLI integration tests for `gat`.
//!
//! These tests create disposable Git repositories and exercise the compiled
//! binary the way a user would. That catches subprocess, path, and worktree
//! behavior that unit tests cannot validate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Returns the compiled `gat` binary produced by Cargo for integration tests.
fn gat() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gat"))
}

/// Builds a unique temporary directory path for an integration test.
fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("gat-{name}-{}-{nanos}", std::process::id()))
}

/// Runs a Git command in `repo` and fails the test with useful output on error.
fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Creates a minimal Git repository with an initial commit.
fn init_repo(name: &str) -> PathBuf {
    let parent = unique_temp_dir(name);
    fs::create_dir_all(&parent).unwrap();
    let repo = parent.join("project");
    fs::create_dir(&repo).unwrap();

    run_git(&repo, &["init", "--initial-branch=master"]);
    run_git(&repo, &["config", "user.email", "test@example.com"]);
    run_git(&repo, &["config", "user.name", "Test User"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "init"]);

    repo
}

/// Runs the test binary in `repo`.
fn run_gat(repo: &Path, args: &[&str]) -> Output {
    Command::new(gat())
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap()
}

/// Runs the test binary with one directory prepended to `PATH`.
#[cfg(unix)]
fn run_gat_with_path(repo: &Path, args: &[&str], extra_path: &Path) -> Output {
    let config_home = repo.parent().unwrap().join("config");
    Command::new(gat())
        .args(args)
        .current_dir(repo)
        .env("PATH", test_path(Some(extra_path)))
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .unwrap()
}

/// Runs the test binary in an arbitrary working directory with one directory prepended to `PATH`.
#[cfg(unix)]
fn run_gat_with_path_in_dir(dir: &Path, args: &[&str], extra_path: &Path) -> Output {
    let config_home = dir.parent().unwrap().join("config");
    Command::new(gat())
        .args(args)
        .current_dir(dir)
        .env("PATH", test_path(Some(extra_path)))
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .unwrap()
}

/// Asserts that a command succeeded and prints captured output on failure.
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "gat failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Quotes a filesystem path for a small bash script.
#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Builds a PATH that finds the compiled test binary before any installed `gat`.
#[cfg(unix)]
fn test_path(extra_front: Option<&Path>) -> std::ffi::OsString {
    let mut entries = Vec::new();
    if let Some(path) = extra_front {
        entries.push(path.to_path_buf());
    }
    entries.push(gat().parent().unwrap().to_path_buf());
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(entries).unwrap()
}

/// Runs a bash script with `gat` resolved to the compiled test binary.
#[cfg(unix)]
fn run_shell(repo: &Path, script: &str, extra_path: Option<&Path>) -> Output {
    Command::new("bash")
        .arg("--noprofile")
        .arg("--norc")
        .arg("-c")
        .arg(script)
        .current_dir(repo)
        .env("PATH", test_path(extra_path))
        .output()
        .unwrap()
}

/// Creates a fake `fzf` executable that deterministically selects `branch`.
#[cfg(unix)]
fn fake_fzf_dir(branch: &str) -> PathBuf {
    let dir = unique_temp_dir("fake-fzf");
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fzf");
    fs::write(
        &script,
        format!("#!/bin/sh\nawk -F '\\t' '$1 == \"{branch}\" {{ print; exit }}'\n"),
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    dir
}

/// Creates a fake `tmux` executable and returns `(bin_dir, log_path)`.
#[cfg(unix)]
fn fake_tmux_dir() -> (PathBuf, PathBuf) {
    let dir = unique_temp_dir("fake-tmux");
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("tmux");
    let log = dir.join("tmux.log");
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> {log}
case "$1" in
  has-session)
    exit 1
    ;;
  new-session)
    printf '%s\n' '%1'
    exit 0
    ;;
  split-window)
    case " $* " in
      *" -h "*)
        printf '%s\n' '%2'
        ;;
      *)
        printf '%s\n' '%3'
        ;;
    esac
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
"#,
            log = shell_quote(&log)
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    (dir, log)
}

/// Creates a fake `tmux` executable whose `has-session` result is configurable.
///
/// When `session_exists` is true, `has-session` exits 0 (the session is live),
/// which lets tests exercise the attach-to-existing path. Returns
/// `(bin_dir, log_path)`.
#[cfg(unix)]
fn fake_tmux_dir_with_session(session_exists: bool) -> (PathBuf, PathBuf) {
    let dir = unique_temp_dir("fake-tmux-cfg");
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("tmux");
    let log = dir.join("tmux.log");
    let has_session_exit = if session_exists { 0 } else { 1 };
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> {log}
case "$1" in
  has-session)
    exit {has_session_exit}
    ;;
  new-session)
    printf '%s\n' '%1'
    exit 0
    ;;
  split-window)
    case " $* " in
      *" -h "*)
        printf '%s\n' '%2'
        ;;
      *)
        printf '%s\n' '%3'
        ;;
    esac
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
"#,
            log = shell_quote(&log),
            has_session_exit = has_session_exit
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    (dir, log)
}

/// Writes a small executable script into `dir`.
#[cfg(unix)]
fn write_executable(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

/// Recursively finds prompt draft files below `root`.
#[cfg(unix)]
fn prompt_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("pre-prompt.md") {
                found.push(path);
            }
        }
    }
    found
}

/// Returns the last stdout line as a path-like string.
#[cfg(unix)]
fn last_stdout_line(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

/// Writes a minimal compose setup for `gat dx` tests.
fn write_compose_files(repo: &Path, override_services: Option<&str>) {
    let compose_dir = repo.join(".docker");
    fs::create_dir_all(&compose_dir).unwrap();
    fs::write(
        compose_dir.join("docker-compose.yml"),
        r#"services:
  base:
    image: example/base
    profiles: ["_disabled"]

  config-generator:
    image: example/config
    profiles: ["_disabled"]

  dp1:
    image: example/dp1
"#,
    )
    .unwrap();

    if let Some(override_services) = override_services {
        fs::write(
            compose_dir.join("docker-compose.override.yml"),
            override_services,
        )
        .unwrap();
    }
}

#[test]
fn dry_run_computes_sibling_ticket_path() {
    let repo = init_repo("dry-run");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");

    let output = run_gat(&repo, &["12345", "--dry-run"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would_create"));
    assert!(stdout.contains("TICKET-12345"));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
    assert!(!expected.exists());
}

#[test]
fn custom_prefix_changes_numeric_ticket() {
    let repo = init_repo("custom-prefix");
    let expected = repo.parent().unwrap().join("project-ABC-12345");

    let output = run_gat(&repo, &["12345", "--prefix", "ABC", "--dry-run"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ABC-12345"));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
    assert!(!expected.exists());
}

#[test]
fn no_prefix_keeps_numeric_ticket_raw() {
    let repo = init_repo("no-prefix");
    let expected = repo.parent().unwrap().join("project-12345");

    let output = run_gat(&repo, &["12345", "--no-prefix", "--dry-run"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("branch 12345"));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
    assert!(!expected.exists());
}

#[test]
fn local_git_config_can_set_ticket_prefix() {
    let repo = init_repo("config-prefix");
    let expected = repo.parent().unwrap().join("project-GAT-12345");
    run_git(&repo, &["config", "gat.ticketPrefix", "GAT"]);

    let output = run_gat(&repo, &["12345", "--dry-run"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GAT-12345"));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
}

#[test]
fn creates_and_reuses_ticket_worktree() {
    let repo = init_repo("create");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");

    let output = run_gat(&repo, &["12345"]);
    assert_success(&output);
    assert!(expected.exists());
    assert!(expected.join("README.md").exists());

    let second = run_gat(&repo, &["12345"]);
    assert_success(&second);
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("existing"));
}

#[test]
fn path_and_list_find_created_worktree() {
    let repo = init_repo("path-list");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    let path = run_gat(&repo, &["path", "12345"]);
    assert_success(&path);
    let actual = PathBuf::from(String::from_utf8_lossy(&path.stdout).trim().to_string());
    assert_eq!(
        fs::canonicalize(actual).unwrap(),
        fs::canonicalize(&expected).unwrap()
    );

    let list = run_gat(&repo, &["list", "--json"]);
    assert_success(&list);
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("\"branch\":\"TICKET-12345\""));
    assert!(stdout.contains("project-TICKET-12345"));
}

#[test]
fn remove_blocks_dirty_worktree_without_force() {
    let repo = init_repo("dirty-remove");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));
    fs::write(expected.join("dirty.txt"), "dirty\n").unwrap();

    let output = run_gat(&repo, &["rm", "12345", "--yes"]);

    assert!(!output.status.success());
    assert!(expected.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("uncommitted changes"));
}

#[test]
fn remove_force_removes_worktree() {
    let repo = init_repo("force-remove");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));
    fs::write(expected.join("dirty.txt"), "dirty\n").unwrap();

    let output = run_gat(&repo, &["rm", "12345", "--yes", "--force"]);

    assert_success(&output);
    assert!(!expected.exists());
}

#[test]
fn shell_format_returns_assignments() {
    let repo = init_repo("shell-format");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");

    let output = run_gat(&repo, &["12345", "--dry-run", "--format", "shell"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GAT_STATUS=ok"));
    assert!(stdout.contains("GAT_PATH="));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
}

#[cfg(unix)]
#[test]
fn shell_integration_shorthand_create_changes_directory() {
    let repo = init_repo("shell-create");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    let script = format!(
        "eval \"$(gat shell-init --shell bash)\"\ncd {}\ngat 12345 >/dev/null\npwd\n",
        shell_quote(&repo)
    );

    let output = run_shell(&repo, &script, None);

    assert_success(&output);
    assert_eq!(
        fs::canonicalize(last_stdout_line(&output)).unwrap(),
        fs::canonicalize(expected).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn shell_integration_go_changes_directory() {
    let repo = init_repo("shell-go");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));
    let script = format!(
        "eval \"$(gat shell-init --shell bash)\"\ncd {}\ngat go 12345 >/dev/null\npwd\n",
        shell_quote(&repo)
    );

    let output = run_shell(&repo, &script, None);

    assert_success(&output);
    assert_eq!(
        fs::canonicalize(last_stdout_line(&output)).unwrap(),
        fs::canonicalize(expected).unwrap()
    );
}

#[test]
fn watch_once_lists_existing_worktrees() {
    let repo = init_repo("watch");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["watch", "--once"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Branch"));
    assert!(stdout.contains("TICKET-12345"));
}

#[test]
fn search_print_outputs_tab_separated_feed() {
    let repo = init_repo("search");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["search", "--print"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TICKET-12345\t"));
    assert!(stdout.contains(&expected.to_string_lossy().to_string()));
}

#[test]
fn search_no_fzf_outputs_feed_without_switching() {
    let repo = init_repo("search-no-fzf");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["search", "--no-fzf", "--format", "shell"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TICKET-12345\t"));
    assert!(!stdout.contains("GAT_PATH="));
}

#[cfg(unix)]
#[test]
fn shell_integration_search_changes_directory_after_fzf_selection() {
    let repo = init_repo("shell-search");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));
    let fake_fzf = fake_fzf_dir("TICKET-12345");
    let script = format!(
        "eval \"$(gat shell-init --shell bash)\"\ncd {}\ngat search >/dev/null\npwd\n",
        shell_quote(&repo)
    );

    let output = run_shell(&repo, &script, Some(&fake_fzf));

    assert_success(&output);
    assert_eq!(
        fs::canonicalize(last_stdout_line(&output)).unwrap(),
        fs::canonicalize(expected).unwrap()
    );
}

#[test]
fn shell_init_guards_search_feed_from_eval() {
    let repo = init_repo("shell-init-search");

    let output = run_gat(&repo, &["shell-init", "--shell", "bash"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--print|--no-fzf|--path|--tmux"));
    assert!(stdout.contains("dx|docker"));
    assert!(stdout.contains("GAT_SHELL_INTEGRATION=1 command gat"));
    assert!(stdout.contains("unset GAT_STATUS"));
}

#[cfg(unix)]
#[test]
fn dx_execs_into_running_container_for_current_worktree() {
    let repo = init_repo("dx-exec");
    write_compose_files(&repo, None);
    let other_worktree = repo.parent().unwrap().join("other-worktree");
    fs::create_dir_all(&other_worktree).unwrap();

    let fake_dir = unique_temp_dir("fake-docker-exec");
    fs::create_dir_all(&fake_dir).unwrap();
    let log_path = fake_dir.join("docker.log");
    write_executable(
        &fake_dir,
        "docker",
        &format!(
            r#"#!/bin/sh
printf '%s|pwd=%s\n' "$*" "$PWD" >> {log}
case "$1" in
  ps)
    printf '%s\n' other-container current-container
    ;;
  inspect)
    case "$4" in
      other-container)
        printf '%s|dp1|/ticket_other\n' {other_root}
        ;;
      current-container)
        printf '%s|dp1|/ticket_current\n' {repo_root}
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  exec)
    exit 0
    ;;
  *)
    echo "unexpected docker command: $*" >&2
    exit 1
    ;;
esac
"#,
            log = shell_quote(&log_path),
            other_root = shell_quote(&other_worktree),
            repo_root = shell_quote(&repo)
        ),
    );

    let output = run_gat_with_path(&repo, &["dx", "bash"], &fake_dir);

    assert_success(&output);
    let log = fs::read_to_string(log_path).unwrap();
    assert!(log.contains("exec -ti current-container bash"));
    assert!(!log.contains("compose run --rm"));
}

#[cfg(unix)]
#[test]
fn dx_falls_back_to_compose_run_from_worktree_compose_dir() {
    let repo = init_repo("dx-fallback");
    write_compose_files(&repo, None);
    let nested = repo.join("nested");
    fs::create_dir_all(&nested).unwrap();

    let fake_dir = unique_temp_dir("fake-docker-fallback");
    fs::create_dir_all(&fake_dir).unwrap();
    let log_path = fake_dir.join("docker.log");
    write_executable(
        &fake_dir,
        "docker",
        &format!(
            r#"#!/bin/sh
printf '%s|pwd=%s\n' "$*" "$PWD" >> {log}
case "$1" in
  ps)
    exit 0
    ;;
  compose)
    exit 0
    ;;
  *)
    echo "unexpected docker command: $*" >&2
    exit 1
    ;;
esac
"#,
            log = shell_quote(&log_path)
        ),
    );

    let output = run_gat_with_path_in_dir(&nested, &["dx"], &fake_dir);

    assert_success(&output);
    let log = fs::read_to_string(log_path).unwrap();
    assert!(log.contains("compose run --rm dp1 bash|pwd="));
    assert!(log.contains(".docker"));
}

#[cfg(unix)]
#[test]
fn dx_doctor_reports_selected_service_and_declared_services() {
    let repo = init_repo("dx-doctor");
    write_compose_files(
        &repo,
        Some(
            r#"services:
  snd1:
    image: example/snd1
"#,
        ),
    );
    run_git(&repo, &["config", "gat.dockerService", "snd1"]);

    let fake_dir = unique_temp_dir("fake-docker-doctor");
    fs::create_dir_all(&fake_dir).unwrap();
    write_executable(
        &fake_dir,
        "docker",
        r#"#!/bin/sh
case "$1" in
  ps)
    exit 0
    ;;
  *)
    echo "unexpected docker command: $*" >&2
    exit 1
    ;;
esac
"#,
    );

    let output = run_gat_with_path(&repo, &["dx", "--doctor"], &fake_dir);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gat dx doctor"));
    assert!(stdout.contains("Selected service: snd1 (git config gat.dockerService)"));
    assert!(stdout.contains("Declared services:"));
    assert!(stdout.contains("  - dp1"));
    assert!(stdout.contains("  - snd1"));
}

#[test]
fn tmux_dry_run_plans_session_without_creating_worktree() {
    let repo = init_repo("tmux-plan");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");

    let output = run_gat(&repo, &["tmux", "12345", "--dry-run", "--no-attach"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tmux plan"));
    assert!(stdout.contains("gat-TICKET-12345"));
    assert!(stdout.contains("codex"));
    assert!(stdout.contains("nvim"));
    assert!(!expected.exists());
}

#[test]
fn tmux_dry_run_supports_json_format() {
    let repo = init_repo("tmux-plan-json");

    let output = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"action\":\"tmux_plan\""));
    assert!(stdout.contains("\"ticket\":\"TICKET-12345\""));
    assert!(stdout.contains("\"session\":\"gat-TICKET-12345\""));
    assert!(stdout.contains("\"attach_command\":\"no attach\""));
}

#[cfg(unix)]
#[test]
fn tmux_creation_uses_stable_shell_session_and_pane_ids() {
    let repo = init_repo("tmux-fake");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    let (fake_tmux, tmux_log) = fake_tmux_dir();

    let output = run_gat_with_path(
        &repo,
        &[
            "tmux",
            "12345",
            "--no-attach",
            "--session",
            "gat-test",
            "--codex-cmd",
            "echo codex",
            "--editor-cmd",
            "echo editor",
        ],
        &fake_tmux,
    );

    assert_success(&output);
    assert!(expected.exists());
    assert!(!expected.join(".gat").exists());
    let prompts = prompt_files(&repo.parent().unwrap().join("config").join("gat"));
    assert_eq!(prompts.len(), 1);
    let prompt = prompts.first().unwrap();
    assert!(fs::read_to_string(prompt).unwrap().contains("Worktree:"));

    let log = fs::read_to_string(tmux_log).unwrap();
    assert!(log.contains("has-session -t gat-test"));
    assert!(log.contains("new-session -d -P -F #{pane_id} -s gat-test -n gat -c"));
    assert!(log.contains("/bin/bash"));
    assert!(log.contains("split-window -h -l 45% -P -F #{pane_id} -t %1 -c"));
    assert!(log.contains("split-window -v -l 35% -P -F #{pane_id} -t %2 -c"));
    assert!(log.contains("send-keys -t %1 GAT_PROMPT_FILE="));
    assert!(log.contains("echo codex C-m"));
    assert!(log.contains("send-keys -t %2 GAT_PROMPT_FILE="));
    assert!(log.contains("echo editor"));
    assert!(log.contains(&expected.to_string_lossy().to_string()));
    assert!(log.contains(&prompt.to_string_lossy().to_string()));
    assert!(log.contains("select-pane -t %1"));
    assert!(!log.contains(".gat/pre-prompt.md"));
    assert!(!log.contains("gat-test:0"));
}

#[test]
fn archive_dry_run_does_not_move_worktree() {
    let repo = init_repo("archive-dry");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["archive", "12345", "--dry-run"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would_archive"));
    assert!(expected.exists());
}

#[test]
fn archive_moves_worktree_to_archive_directory() {
    let repo = init_repo("archive");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    let archive_root = repo.parent().unwrap().join("archives");
    let archived = archive_root.join("TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(
        &repo,
        &[
            "archive",
            "12345",
            "--yes",
            "--archive-dir",
            archive_root.to_str().unwrap(),
        ],
    );

    assert_success(&output);
    assert!(!expected.exists());
    assert!(archived.exists());
    assert!(archived.join("README.md").exists());
}

#[test]
fn version_flag_prints_version() {
    let repo = init_repo("version");

    let output = run_gat(&repo, &["--version"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("gat "));
    assert!(stdout.trim().split(' ').nth(1).is_some());
}

#[test]
fn list_includes_idle_column() {
    let repo = init_repo("list-idle");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["list"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Idle"));
    // A freshly created worktree should report 0 days idle.
    assert!(stdout.contains("0d"));
}

#[test]
fn list_json_includes_idle_days() {
    let repo = init_repo("list-idle-json");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["list", "--json"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"idle_days\":"));
}

#[test]
fn remove_cleans_up_metadata_entry() {
    let repo = init_repo("meta-cleanup");
    let metadata_file = repo.join(".git").join("gat-metadata.json");
    assert_success(&run_gat(&repo, &["12345"]));
    assert!(metadata_file.exists());
    let before = fs::read_to_string(&metadata_file).unwrap();
    assert!(before.contains("TICKET-12345"));

    assert_success(&run_gat(&repo, &["rm", "12345", "--yes"]));

    let after = fs::read_to_string(&metadata_file).unwrap();
    assert!(!after.contains("project-TICKET-12345"));
}

#[test]
fn prune_older_than_zero_is_rejected() {
    let repo = init_repo("prune-zero");

    let output = run_gat(&repo, &["prune", "--older-than", "0", "--dry-run"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("positive integer"));
}

#[test]
fn prune_older_than_keeps_fresh_worktrees() {
    let repo = init_repo("prune-fresh");
    let expected = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    // A worktree created moments ago must not be pruned by a 30-day window.
    let output = run_gat(&repo, &["prune", "--older-than", "30", "--dry-run"]);

    assert_success(&output);
    assert!(expected.exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("TICKET-12345"));
}

#[test]
fn list_shows_change_stats_for_dirty_worktree() {
    let repo = init_repo("list-changes");
    let worktree = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    // Modify a tracked file and add an untracked one.
    fs::write(worktree.join("README.md"), "changed\nsecond\n").unwrap();
    fs::write(worktree.join("extra.txt"), "new\n").unwrap();

    let output = run_gat(&repo, &["list"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Changes"));
    // Two changed paths: the modified README and the untracked file.
    assert!(stdout.contains("2f"));
}

#[test]
fn list_json_reports_change_counts() {
    let repo = init_repo("list-changes-json");
    let worktree = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));

    fs::write(worktree.join("README.md"), "changed line\n").unwrap();

    let output = run_gat(&repo, &["list", "--json"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"changed_files\":"));
    assert!(stdout.contains("\"insertions\":"));
    assert!(stdout.contains("\"deletions\":"));
    assert!(stdout.contains("\"dirty\":true"));
}

#[test]
fn list_fast_skips_change_stats() {
    let repo = init_repo("list-fast");
    let worktree = repo.parent().unwrap().join("project-TICKET-12345");
    assert_success(&run_gat(&repo, &["12345"]));
    fs::write(worktree.join("README.md"), "changed\n").unwrap();

    let output = run_gat(&repo, &["list", "--fast", "--json"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Fast mode skips expensive checks, so changes report as zero/clean.
    assert!(stdout.contains("\"dirty\":false"));
    assert!(stdout.contains("\"changed_files\":0"));
}

#[test]
fn switch_errors_when_no_worktree_exists() {
    let repo = init_repo("switch-missing");

    let output = run_gat(&repo, &["switch", "12345"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no worktree found"));
    assert!(stderr.contains("gat new"));
}

#[test]
fn switch_dry_run_plans_session_for_existing_worktree() {
    let repo = init_repo("switch-plan");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["switch", "12345", "--dry-run", "--format", "json"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"action\":\"switch_plan\""));
    assert!(stdout.contains("\"target\":\"TICKET-12345\""));
    assert!(stdout.contains("\"session\":\"gat-TICKET-12345\""));
    assert!(stdout.contains("\"resolved_action\":\"create_session\""));
}

#[test]
fn switch_dry_run_reports_missing_worktree_action() {
    let repo = init_repo("switch-plan-missing");

    let output = run_gat(&repo, &["switch", "12345", "--dry-run"]);

    // Dry-run never errors; it reports the resolved action instead.
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("action: no_worktree"));
}

#[cfg(unix)]
#[test]
fn switch_creates_session_when_worktree_exists_but_session_absent() {
    let repo = init_repo("switch-create");
    assert_success(&run_gat(&repo, &["12345"]));
    let (fake_tmux, tmux_log) = fake_tmux_dir_with_session(false);

    let output = run_gat_with_path(
        &repo,
        &[
            "switch",
            "12345",
            "--no-attach",
            "--codex-cmd",
            "echo codex",
            "--editor-cmd",
            "echo editor",
        ],
        &fake_tmux,
    );

    assert_success(&output);
    let log = fs::read_to_string(tmux_log).unwrap();
    // Session was absent, so it must be created with the gat layout.
    assert!(log.contains("has-session -t gat-TICKET-12345"));
    assert!(log.contains("new-session -d -P -F #{pane_id} -s gat-TICKET-12345"));
    assert!(log.contains("split-window -h"));
    assert!(log.contains("split-window -v"));
}

#[cfg(unix)]
#[test]
fn switch_attaches_to_existing_session_without_recreating() {
    let repo = init_repo("switch-attach");
    assert_success(&run_gat(&repo, &["12345"]));
    let (fake_tmux, tmux_log) = fake_tmux_dir_with_session(true);

    let output = run_gat_with_path(&repo, &["switch", "12345"], &fake_tmux);

    assert_success(&output);
    let log = fs::read_to_string(tmux_log).unwrap();
    assert!(log.contains("has-session -t gat-TICKET-12345"));
    // Session already exists: attach (or switch-client if inside tmux).
    assert!(
        log.contains("attach-session -t gat-TICKET-12345")
            || log.contains("switch-client -t gat-TICKET-12345")
    );
    assert!(!log.contains("new-session"));
    assert!(!log.contains("split-window"));
}

#[test]
fn new_with_description_records_it() {
    let repo = init_repo("desc-new");
    assert_success(&run_gat(
        &repo,
        &["12345", "--description", "fix login redirect"],
    ));

    let output = run_gat(&repo, &["describe", "12345"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fix login redirect"));
}

#[test]
fn describe_sets_and_shows_description() {
    let repo = init_repo("desc-set");
    assert_success(&run_gat(&repo, &["12345"]));

    // Set a multi-word description as trailing positional words.
    let set = run_gat(&repo, &["describe", "12345", "add", "OAuth", "support"]);
    assert_success(&set);

    let show = run_gat(&repo, &["describe", "12345"]);
    assert_success(&show);
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("add OAuth support"));
}

#[test]
fn describe_clear_removes_description() {
    let repo = init_repo("desc-clear");
    assert_success(&run_gat(
        &repo,
        &["12345", "--description", "temporary note"],
    ));

    assert_success(&run_gat(&repo, &["describe", "12345", "--clear"]));

    let output = run_gat(&repo, &["describe", "12345"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("no description"));
}

#[test]
fn describe_errors_on_missing_worktree() {
    let repo = init_repo("desc-missing");

    let output = run_gat(&repo, &["describe", "12345", "some text"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no worktree found"));
}

#[test]
fn list_shows_description_column() {
    let repo = init_repo("desc-list");
    assert_success(&run_gat(
        &repo,
        &["12345", "--description", "payment refactor"],
    ));

    let output = run_gat(&repo, &["list"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("payment refactor"));
}

#[test]
fn list_json_includes_description() {
    let repo = init_repo("desc-list-json");
    assert_success(&run_gat(&repo, &["12345", "--description", "cache layer"]));

    let output = run_gat(&repo, &["list", "--json"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"description\":\"cache layer\""));
}

#[test]
fn tmux_session_name_includes_description() {
    let repo = init_repo("session-desc");
    assert_success(&run_gat(
        &repo,
        &["12345", "--description", "fix login redirect"],
    ));

    let output = run_gat(&repo, &["tmux", "12345", "--dry-run", "--no-attach"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gat-TICKET-12345-fix-login-redirect"));
}

#[test]
fn switch_session_name_includes_description() {
    let repo = init_repo("switch-session-desc");
    assert_success(&run_gat(&repo, &["12345", "--description", "add OAuth"]));

    let output = run_gat(&repo, &["switch", "12345", "--dry-run", "--format", "json"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gat-TICKET-12345-add-OAuth"));
}

#[test]
fn tmux_session_name_is_plain_without_description() {
    let repo = init_repo("session-plain");
    assert_success(&run_gat(&repo, &["12345"]));

    let output = run_gat(&repo, &["tmux", "12345", "--dry-run", "--no-attach"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // No description set: the session name stays the bare gat-<ticket> form.
    assert!(stdout.contains("session: gat-TICKET-12345\n"));
}

#[test]
fn progress_goes_to_stderr_not_stdout() {
    let repo = init_repo("progress-stream");

    let output = run_gat(&repo, &["12345"]);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Progress lines are prefixed with "gat:" and must stay on stderr so the
    // shell integration can safely parse stdout.
    assert!(!stdout.contains("gat: "));
    assert!(stderr.contains("git worktree created"));
}

#[test]
fn gat_quiet_suppresses_progress() {
    let repo = init_repo("progress-quiet");

    let output = Command::new(gat())
        .args(["12345"])
        .current_dir(&repo)
        .env("GAT_QUIET", "1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("gat: "));
}

#[cfg(unix)]
#[test]
fn sessions_lists_gat_sessions_via_fake_tmux() {
    let repo = init_repo("sessions-list");
    // Fake tmux that reports two sessions, one gat-managed and one unrelated.
    let dir = unique_temp_dir("fake-tmux-sessions");
    fs::create_dir_all(&dir).unwrap();
    write_executable(
        &dir,
        "tmux",
        r#"#!/bin/sh
case "$1" in
  list-sessions)
    printf '%s\n' 'gat-TICKET-12345-fix-login	1	3	TICKET-12345	/repo/wt	fix login bug'
    printf '%s\n' 'work	0	1			'
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
"#,
    );

    let output = run_gat_with_path(&repo, &["sessions", "--format", "json"], &dir);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Only the gat-managed session is reported.
    assert!(stdout.contains("\"name\":\"gat-TICKET-12345-fix-login\""));
    assert!(stdout.contains("\"branch\":\"TICKET-12345\""));
    assert!(stdout.contains("\"description\":\"fix login bug\""));
    assert!(!stdout.contains("\"name\":\"work\""));
}

#[cfg(unix)]
#[test]
fn sessions_reports_none_when_server_absent() {
    let repo = init_repo("sessions-none");
    // Fake tmux that fails list-sessions, as when no server is running.
    let dir = unique_temp_dir("fake-tmux-no-server");
    fs::create_dir_all(&dir).unwrap();
    write_executable(
        &dir,
        "tmux",
        r#"#!/bin/sh
case "$1" in
  list-sessions)
    echo "no server running on /tmp/tmux" >&2
    exit 1
    ;;
  *)
    exit 0
    ;;
esac
"#,
    );

    let output = run_gat_with_path(&repo, &["sessions"], &dir);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No gat tmux sessions"));
}

#[test]
fn tmux_layout_preset_changes_plan_geometry() {
    let repo = init_repo("layout-preset");

    // Default (classic) is 55% left.
    let default_plan = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );
    assert_success(&default_plan);
    let default_stdout = String::from_utf8_lossy(&default_plan.stdout);
    assert!(default_stdout.contains("\"left_width\":55"));

    // ai-focus is 70% left.
    let ai_plan = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--layout",
            "ai-focus",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );
    assert_success(&ai_plan);
    let ai_stdout = String::from_utf8_lossy(&ai_plan.stdout);
    assert!(ai_stdout.contains("\"left_width\":70"));
    assert!(ai_stdout.contains("\"bottom_height\":40"));
}

#[test]
fn tmux_editor_focus_preset_focuses_editor() {
    let repo = init_repo("layout-editor");

    let output = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--layout",
            "editor-focus",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"left_width\":35"));
    assert!(stdout.contains("\"focus_left\":false"));
}

#[test]
fn tmux_layout_via_git_config() {
    let repo = init_repo("layout-gitconfig");
    run_git(&repo, &["config", "gat.tmuxLayout", "wide"]);

    let output = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // wide is an even 50/50 split.
    assert!(stdout.contains("\"left_width\":50"));
}

#[test]
fn tmux_explicit_width_overrides_git_layout_preset() {
    let repo = init_repo("layout-override");
    run_git(&repo, &["config", "gat.tmuxLayout", "ai-focus"]);
    run_git(&repo, &["config", "gat.tmuxLeftWidth", "42"]);

    let output = run_gat(
        &repo,
        &[
            "tmux",
            "12345",
            "--dry-run",
            "--no-attach",
            "--format",
            "json",
        ],
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Explicit left width wins over the preset baseline.
    assert!(stdout.contains("\"left_width\":42"));
    // bottom_height still comes from the ai-focus preset.
    assert!(stdout.contains("\"bottom_height\":40"));
}

#[cfg(not(feature = "tui"))]
#[test]
fn ui_reports_unavailable_without_feature() {
    let repo = init_repo("ui-unavailable");

    let output = run_gat(&repo, &["ui"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not available"));
    assert!(stderr.contains("--features tui"));
}

/// Runs gat with an isolated XDG_CONFIG_HOME so config tests do not touch the
/// real user config.
#[cfg(unix)]
fn run_gat_with_config_home(repo: &Path, args: &[&str], config_home: &Path) -> Output {
    Command::new(gat())
        .args(args)
        .current_dir(repo)
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn config_init_and_get_set_roundtrip() {
    let repo = init_repo("config-roundtrip");
    let cfg = repo.parent().unwrap().join("xdg");

    let init = run_gat_with_config_home(&repo, &["config", "init"], &cfg);
    assert_success(&init);
    assert!(cfg.join("gat").join("config.toml").exists());

    let set = run_gat_with_config_home(&repo, &["config", "set", "ticket_prefix", "ABC"], &cfg);
    assert_success(&set);

    let get = run_gat_with_config_home(&repo, &["config", "get", "ticket_prefix"], &cfg);
    assert_success(&get);
    assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "ABC");
}

#[cfg(unix)]
#[test]
fn config_set_rejects_invalid_value() {
    let repo = init_repo("config-invalid");
    let cfg = repo.parent().unwrap().join("xdg");

    let output =
        run_gat_with_config_home(&repo, &["config", "set", "tmux.left_width", "150"], &cfg);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("0-100"));
}

#[cfg(unix)]
#[test]
fn config_init_refuses_overwrite_without_force() {
    let repo = init_repo("config-init-guard");
    let cfg = repo.parent().unwrap().join("xdg");

    assert_success(&run_gat_with_config_home(&repo, &["config", "init"], &cfg));
    let second = run_gat_with_config_home(&repo, &["config", "init"], &cfg);
    assert!(!second.status.success());
    let forced = run_gat_with_config_home(&repo, &["config", "init", "--force"], &cfg);
    assert_success(&forced);
}

#[cfg(unix)]
#[test]
fn config_list_outputs_keys() {
    let repo = init_repo("config-list");
    let cfg = repo.parent().unwrap().join("xdg");

    let output = run_gat_with_config_home(&repo, &["config", "list"], &cfg);

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tmux.left_width = 55"));
    assert!(stdout.contains("tmux.codex_cmd = codex"));
}

#[cfg(unix)]
#[test]
fn template_copies_symlinks_and_runs_on_new() {
    let repo = init_repo("template-apply");
    let cfg = repo.parent().unwrap().join("xdg");

    // A source file (gitignored-style) and a shared dir live in the primary worktree.
    fs::write(repo.join(".env.example"), "TOKEN=abc\n").unwrap();
    fs::create_dir(repo.join("deps")).unwrap();
    fs::write(repo.join("deps").join("lib.txt"), "shared\n").unwrap();

    // Write a config file with a default template.
    let gat_dir = cfg.join("gat");
    fs::create_dir_all(&gat_dir).unwrap();
    fs::write(
        gat_dir.join("config.toml"),
        "[template.default]\ncopy = [\".env.example\"]\nsymlink = [\"deps\"]\nrun = [\"echo ok > setup_ran.txt\"]\n",
    )
    .unwrap();

    let output = run_gat_with_config_home(&repo, &["12345"], &cfg);
    assert_success(&output);

    let worktree = repo.parent().unwrap().join("project-TICKET-12345");
    // Copied file is a real copy.
    assert!(worktree.join(".env.example").exists());
    assert_eq!(
        fs::read_to_string(worktree.join(".env.example")).unwrap(),
        "TOKEN=abc\n"
    );
    // Symlinked dir resolves to the primary worktree's content.
    let linked = worktree.join("deps");
    assert!(linked.exists());
    assert!(fs::symlink_metadata(&linked)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(linked.join("lib.txt")).unwrap(),
        "shared\n"
    );
    // Run command executed in the worktree.
    assert!(worktree.join("setup_ran.txt").exists());
}

#[cfg(unix)]
#[test]
fn no_template_flag_skips_default_template() {
    let repo = init_repo("template-skip");
    let cfg = repo.parent().unwrap().join("xdg");
    fs::write(repo.join(".env.example"), "X=1\n").unwrap();
    let gat_dir = cfg.join("gat");
    fs::create_dir_all(&gat_dir).unwrap();
    fs::write(
        gat_dir.join("config.toml"),
        "[template.default]\ncopy = [\".env.example\"]\n",
    )
    .unwrap();

    let output = run_gat_with_config_home(&repo, &["12345", "--no-template"], &cfg);
    assert_success(&output);

    let worktree = repo.parent().unwrap().join("project-TICKET-12345");
    assert!(!worktree.join(".env.example").exists());
}

#[cfg(unix)]
#[test]
fn unknown_template_is_an_error() {
    let repo = init_repo("template-unknown");
    let cfg = repo.parent().unwrap().join("xdg");

    let output = run_gat_with_config_home(&repo, &["12345", "--template", "ghost"], &cfg);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("template 'ghost' is not configured"));
}

/// Creates a worktree for `ticket` and adds a committed file on its branch.
#[cfg(unix)]
fn worktree_with_commit(repo: &Path, ticket: &str, file: &str) -> PathBuf {
    assert_success(&run_gat(repo, &[ticket, "--no-prefix"]));
    let wt = repo.parent().unwrap().join(format!("project-{ticket}"));
    fs::write(wt.join(file), "feature work\n").unwrap();
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert!(add.status.success());
    let commit = Command::new("git")
        .args(["commit", "-m", "add feature"])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert!(commit.status.success());
    wt
}

#[cfg(unix)]
#[test]
fn merge_dry_run_does_not_change_anything() {
    let repo = init_repo("merge-dry");
    let wt = worktree_with_commit(&repo, "111", "feature.txt");

    let output = run_gat(
        &repo,
        &["merge", "111", "--no-prefix", "--dry-run", "--json"],
    );

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"action\":\"merge_plan\""));
    assert!(stdout.contains("\"branch\":\"111\""));
    // Worktree still exists; main does not yet have the file.
    assert!(wt.exists());
    assert!(!repo.join("feature.txt").exists());
}

#[cfg(unix)]
#[test]
fn merge_applies_branch_to_default() {
    let repo = init_repo("merge-apply");
    let wt = worktree_with_commit(&repo, "111", "feature.txt");

    let output = run_gat(&repo, &["merge", "111", "--no-prefix", "--yes"]);

    assert_success(&output);
    // The feature file now exists on the primary worktree (default branch).
    assert!(repo.join("feature.txt").exists());
    // No cleanup requested, so the worktree remains.
    assert!(wt.exists());
}

#[cfg(unix)]
#[test]
fn merge_cleanup_removes_worktree_and_branch() {
    let repo = init_repo("merge-cleanup");
    let wt = worktree_with_commit(&repo, "111", "feature.txt");

    let output = run_gat(
        &repo,
        &["merge", "111", "--no-prefix", "--cleanup", "--yes"],
    );

    assert_success(&output);
    assert!(repo.join("feature.txt").exists());
    assert!(!wt.exists());

    // Branch 111 should be gone.
    let branches = Command::new("git")
        .args(["branch", "--list", "111"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
}

#[cfg(unix)]
#[test]
fn merge_refuses_dirty_worktree() {
    let repo = init_repo("merge-dirty");
    let wt = worktree_with_commit(&repo, "111", "feature.txt");
    fs::write(wt.join("uncommitted.txt"), "wip\n").unwrap();

    let output = run_gat(&repo, &["merge", "111", "--no-prefix", "--yes"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("uncommitted changes"));
}

#[cfg(unix)]
#[test]
fn merge_errors_on_missing_worktree() {
    let repo = init_repo("merge-missing");

    let output = run_gat(&repo, &["merge", "999", "--no-prefix", "--yes"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no worktree found"));
}

#[cfg(unix)]
#[test]
fn merge_refuses_when_primary_on_other_branch() {
    let repo = init_repo("merge-primary-branch");
    let _wt = worktree_with_commit(&repo, "111", "feature.txt");

    // Move the primary worktree off the default branch.
    let co = Command::new("git")
        .args(["checkout", "-b", "scratch"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(co.status.success());

    let output = run_gat(&repo, &["merge", "111", "--no-prefix", "--yes"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not master")
            || stderr.contains("not main")
            || stderr.contains("check out")
    );
}
