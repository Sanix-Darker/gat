//! Docker helpers for worktree-scoped container access.
//!
//! The `gat dx` workflow is intentionally opinionated around Git worktrees:
//! it looks for running containers whose bind mount source matches the current
//! worktree root, then falls back to `docker compose run` from the worktree's
//! compose directory when nothing is running yet.

use crate::error::{GatError, Result};
use crate::git::{self, Repo};
use crate::output::path_string;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const DEFAULT_COMPOSE_DIR: &str = ".docker";
const DEFAULT_WORKTREE_MOUNT: &str = "/cbr/apps";
const DEFAULT_SERVICE: &str = "dp1";
const COMPOSE_FILE: &str = "docker-compose.yml";
const COMPOSE_OVERRIDE_FILE: &str = "docker-compose.override.yml";
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";

#[derive(Clone, Debug)]
struct ComposeService {
    name: String,
    disabled: bool,
}

#[derive(Clone, Debug)]
struct RunningContainer {
    id: String,
    name: String,
    service: Option<String>,
    worktree_root: PathBuf,
}

#[derive(Clone, Debug)]
struct ServiceChoice {
    name: String,
    source: &'static str,
}

#[derive(Debug)]
struct DockerPlan {
    worktree_root: PathBuf,
    compose_dir: PathBuf,
    compose_file_exists: bool,
    override_file_exists: bool,
    worktree_mount: String,
    declared_services: Vec<ComposeService>,
    running_containers: Vec<RunningContainer>,
    selected_service: Option<ServiceChoice>,
    exec_target: Option<RunningContainer>,
    compose_target: Option<ServiceChoice>,
    issue: Option<String>,
}

/// Executes a worktree-scoped Docker shell or command.
pub fn dx(repo: &Repo, service_override: Option<&str>, command: &[String]) -> Result<()> {
    if !command_exists("docker") {
        return Err(GatError::NotFound("docker not found on PATH".into()));
    }

    let plan = build_plan(repo, service_override)?;
    let command = if command.is_empty() {
        vec!["bash".to_string()]
    } else {
        command.to_vec()
    };

    if let Some(container) = plan.exec_target.as_ref() {
        let mut docker = Command::new("docker");
        docker.arg("exec").arg("-ti").arg(&container.id);
        docker.args(&command);
        let status = docker.status()?;
        return exit_status_ok(status, "docker exec");
    }

    if let Some(service) = plan.compose_target.as_ref() {
        let mut docker = Command::new("docker");
        docker.current_dir(&plan.compose_dir);
        docker
            .arg("compose")
            .arg("run")
            .arg("--rm")
            .arg(&service.name);
        docker.args(&command);
        let status = docker.status()?;
        return exit_status_ok(status, "docker compose run");
    }

    Err(GatError::Io(plan.issue.unwrap_or_else(|| {
        "unable to determine docker action for the current worktree".into()
    })))
}

/// Returns a human-readable diagnosis for `gat dx`.
pub fn dx_doctor(repo: &Repo, service_override: Option<&str>) -> Result<String> {
    let plan = build_plan(repo, service_override)?;
    let mut out = String::new();

    out.push_str("gat dx doctor\n");
    out.push_str(&format!(
        "Worktree root: {}\n",
        path_string(&plan.worktree_root)
    ));
    out.push_str(&format!(
        "Compose dir: {}\n",
        path_string(&plan.compose_dir)
    ));
    out.push_str(&format!(
        "Compose file: {}\n",
        yes_no(plan.compose_file_exists)
    ));
    out.push_str(&format!(
        "Override file: {}\n",
        yes_no(plan.override_file_exists)
    ));
    out.push_str(&format!(
        "Docker available: {}\n",
        yes_no(command_exists("docker"))
    ));
    out.push_str(&format!("Worktree mount: {}\n", plan.worktree_mount));

    match plan.selected_service.as_ref() {
        Some(choice) => out.push_str(&format!(
            "Selected service: {} ({})\n",
            choice.name, choice.source
        )),
        None => out.push_str("Selected service: unresolved\n"),
    }

    if plan.declared_services.is_empty() {
        out.push_str("Declared services: none\n");
    } else {
        out.push_str("Declared services:\n");
        for service in &plan.declared_services {
            if service.disabled {
                out.push_str(&format!("  - {} (disabled)\n", service.name));
            } else {
                out.push_str(&format!("  - {}\n", service.name));
            }
        }
    }

    if plan.running_containers.is_empty() {
        out.push_str("Running containers for worktree: none\n");
    } else {
        out.push_str("Running containers for worktree:\n");
        for container in &plan.running_containers {
            out.push_str(&format!(
                "  - id={} name={} service={} root={}\n",
                container.id,
                container.name,
                container.service.as_deref().unwrap_or("<unknown>"),
                path_string(&container.worktree_root)
            ));
        }
    }

    match (plan.exec_target.as_ref(), plan.compose_target.as_ref()) {
        (Some(container), _) => out.push_str(&format!(
            "Action: docker exec -ti {} <command>\n",
            container.id
        )),
        (None, Some(service)) => out.push_str(&format!(
            "Action: docker compose run --rm {} <command>\n",
            service.name
        )),
        (None, None) => out.push_str("Action: unresolved\n"),
    }

    if let Some(issue) = plan.issue.as_deref() {
        out.push_str(&format!("Issue: {issue}\n"));
    }

    Ok(out)
}

fn build_plan(repo: &Repo, service_override: Option<&str>) -> Result<DockerPlan> {
    let worktree_root = normalize_path(&repo.current_root)?;
    let compose_dir = resolve_compose_dir(repo)?;
    let compose_file_exists = compose_dir.join(COMPOSE_FILE).is_file();
    let override_file_exists = compose_dir.join(COMPOSE_OVERRIDE_FILE).is_file();
    let worktree_mount = resolve_worktree_mount(repo)?;
    let declared_services = if compose_file_exists || override_file_exists {
        list_declared_services(&compose_dir)?
    } else {
        Vec::new()
    };

    let running_containers = if command_exists("docker") {
        list_running_containers(&worktree_root, &worktree_mount)?
    } else {
        Vec::new()
    };

    let selected_service = resolve_service_choice(repo, service_override, &declared_services)?;
    let exec_target = select_exec_target(&running_containers, selected_service.as_ref());

    let mut compose_target = None;
    let mut issue = None;

    if exec_target.is_none() {
        if !compose_file_exists {
            issue = Some(format!(
                "missing compose file at {}",
                path_string(&compose_dir.join(COMPOSE_FILE))
            ));
        } else if let Some(choice) = selected_service.clone() {
            if service_declared(&declared_services, &choice.name) {
                compose_target = Some(choice);
            } else {
                issue = Some(format!(
                    "service {} is not declared in {}",
                    choice.name,
                    path_string(&compose_dir)
                ));
            }
        } else if running_containers.len() > 1 {
            issue = Some(
                "multiple containers match this worktree; use `gat dx --service <name>`"
                    .to_string(),
            );
        } else if running_containers.is_empty() {
            issue = Some(select_service_help(&declared_services));
        }
    }

    Ok(DockerPlan {
        worktree_root,
        compose_dir,
        compose_file_exists,
        override_file_exists,
        worktree_mount,
        declared_services,
        running_containers,
        selected_service,
        exec_target,
        compose_target,
        issue,
    })
}

fn resolve_compose_dir(repo: &Repo) -> Result<PathBuf> {
    if let Some(path) = non_empty_env("GAT_DOCKER_COMPOSE_DIR") {
        return absolutize_under_worktree(&repo.current_root, Path::new(&path));
    }
    if let Some(path) = git::config_get(repo, "gat.dockerComposeDir")? {
        return absolutize_under_worktree(&repo.current_root, &PathBuf::from(path));
    }
    absolutize_under_worktree(&repo.current_root, &PathBuf::from(DEFAULT_COMPOSE_DIR))
}

fn resolve_worktree_mount(repo: &Repo) -> Result<String> {
    if let Some(value) = non_empty_env("GAT_DOCKER_WORKTREE_MOUNT") {
        return Ok(value);
    }
    if let Some(value) = git::config_get(repo, "gat.dockerWorktreeMount")? {
        return Ok(value);
    }
    Ok(DEFAULT_WORKTREE_MOUNT.to_string())
}

fn resolve_service_choice(
    repo: &Repo,
    service_override: Option<&str>,
    declared_services: &[ComposeService],
) -> Result<Option<ServiceChoice>> {
    if let Some(service) = service_override.filter(|value| !value.is_empty()) {
        return Ok(Some(ServiceChoice {
            name: service.to_string(),
            source: "cli --service",
        }));
    }
    if let Some(service) = non_empty_env("GAT_DOCKER_SERVICE") {
        return Ok(Some(ServiceChoice {
            name: service,
            source: "GAT_DOCKER_SERVICE",
        }));
    }
    if let Some(service) = git::config_get(repo, "gat.dockerService")? {
        return Ok(Some(ServiceChoice {
            name: service,
            source: "git config gat.dockerService",
        }));
    }
    if declared_services
        .iter()
        .any(|service| service.name == DEFAULT_SERVICE)
    {
        return Ok(Some(ServiceChoice {
            name: DEFAULT_SERVICE.to_string(),
            source: "default dp1",
        }));
    }

    let runnable = declared_services
        .iter()
        .filter(|service| !service.disabled)
        .collect::<Vec<_>>();
    if runnable.len() == 1 {
        return Ok(Some(ServiceChoice {
            name: runnable[0].name.clone(),
            source: "only runnable service",
        }));
    }

    Ok(None)
}

fn select_exec_target(
    containers: &[RunningContainer],
    selected_service: Option<&ServiceChoice>,
) -> Option<RunningContainer> {
    if let Some(choice) = selected_service {
        if let Some(container) = containers
            .iter()
            .find(|container| container.service.as_deref() == Some(choice.name.as_str()))
        {
            return Some(container.clone());
        }
    }

    if selected_service.is_none() && containers.len() == 1 {
        return containers.first().cloned();
    }

    None
}

fn list_running_containers(
    worktree_root: &Path,
    worktree_mount: &str,
) -> Result<Vec<RunningContainer>> {
    let ids = docker_output(&["ps", "-q"])?;
    let mut containers = Vec::new();

    for container_id in ids.lines().map(str::trim).filter(|value| !value.is_empty()) {
        let template = format!(
            "{{{{range .Mounts}}}}{{{{if eq .Destination \"{worktree_mount}\"}}}}{{{{.Source}}}}{{{{end}}}}{{{{end}}}}|{{{{with .Config.Labels}}}}{{{{index . \"{COMPOSE_SERVICE_LABEL}\"}}}}{{{{end}}}}|{{{{.Name}}}}"
        );
        let inspect = docker_output(&["inspect", "--format", &template, container_id])?;
        let mut parts = inspect.trim().splitn(3, '|');
        let Some(source) = parts.next() else {
            continue;
        };
        if source.is_empty() {
            continue;
        }
        let source_path = normalize_path(Path::new(source))?;
        if source_path != worktree_root {
            continue;
        }
        let service = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let name = parts
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();
        containers.push(RunningContainer {
            id: container_id.to_string(),
            name,
            service,
            worktree_root: source_path,
        });
    }

    Ok(containers)
}

fn docker_output(args: &[&str]) -> Result<String> {
    let output = Command::new("docker").args(args).output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(GatError::Io(format!(
        "docker {} failed\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn list_declared_services(compose_dir: &Path) -> Result<Vec<ComposeService>> {
    let mut services = Vec::new();
    let mut seen = HashSet::new();

    for file_name in [COMPOSE_FILE, COMPOSE_OVERRIDE_FILE] {
        let compose_file = compose_dir.join(file_name);
        if !compose_file.is_file() {
            continue;
        }

        match parse_declared_services_yaml(&compose_file) {
            Ok(parsed) => {
                for service in parsed {
                    if seen.insert(service.name.clone()) {
                        services.push(service);
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "Failed to parse {} as YAML, falling back to simple parser: {}",
                    file_name,
                    e
                );
                // Fallback to simple parser for backwards compatibility
                for service in parse_declared_services_simple(&compose_file)? {
                    if seen.insert(service.name.clone()) {
                        services.push(service);
                    }
                }
            }
        }
    }

    Ok(services)
}

/// Parses docker-compose.yml using proper YAML parser.
fn parse_declared_services_yaml(path: &Path) -> Result<Vec<ComposeService>> {
    use serde_yaml::Value;

    let content = fs::read_to_string(path)?;
    let yaml: Value =
        serde_yaml::from_str(&content).map_err(|e| GatError::Io(format!("invalid YAML: {e}")))?;

    let mut services = Vec::new();

    if let Some(services_obj) = yaml.get("services").and_then(|v| v.as_mapping()) {
        for (key, value) in services_obj {
            let Some(name) = key.as_str() else {
                continue;
            };

            let mut disabled = false;

            // Check if service has "_disabled" profile
            if let Some(profiles) = value.get("profiles").and_then(|v| v.as_sequence()) {
                disabled = profiles
                    .iter()
                    .any(|p| p.as_str().is_some_and(|s| s.contains("_disabled")));
            }

            services.push(ComposeService {
                name: name.to_string(),
                disabled,
            });
        }
    }

    Ok(services)
}

/// Simple fallback parser for when YAML parsing fails.
fn parse_declared_services_simple(path: &Path) -> Result<Vec<ComposeService>> {
    let content = fs::read_to_string(path)?;
    let mut in_services = false;
    let mut current_name: Option<String> = None;
    let mut current_disabled = false;
    let mut services = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if !in_services {
            if trimmed == "services:" {
                in_services = true;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if !line.starts_with(' ') && !line.starts_with('\t') {
            if let Some(name) = current_name.take() {
                services.push(ComposeService {
                    name,
                    disabled: current_disabled,
                });
            }
            break;
        }

        if let Some(name) = parse_service_header(line) {
            if let Some(previous) = current_name.replace(name) {
                services.push(ComposeService {
                    name: previous,
                    disabled: current_disabled,
                });
            }
            current_disabled = false;
            continue;
        }

        if current_name.is_some() && line.contains("_disabled") {
            current_disabled = true;
        }
    }

    if let Some(name) = current_name.take() {
        services.push(ComposeService {
            name,
            disabled: current_disabled,
        });
    }

    Ok(services)
}

fn parse_service_header(line: &str) -> Option<String> {
    if !line.starts_with("  ") || line.starts_with("    ") {
        return None;
    }
    let trimmed = line.trim();
    let name = trimmed.strip_suffix(':')?;
    if name.is_empty() {
        return None;
    }
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        Some(name.to_string())
    } else {
        None
    }
}

fn absolutize_under_worktree(worktree_root: &Path, path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(worktree_root.join(path))
    }
}

fn normalize_path(path: &Path) -> Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(_) if path.is_absolute() => Ok(path.to_path_buf()),
        Err(_) => Ok(env::current_dir()?.join(path)),
    }
}

fn service_declared(services: &[ComposeService], name: &str) -> bool {
    services.iter().any(|service| service.name == name)
}

fn select_service_help(services: &[ComposeService]) -> String {
    if services.is_empty() {
        return format!(
            "no docker service could be selected automatically; add {COMPOSE_FILE} under the worktree or pass `gat dx --service <name>`"
        );
    }

    let available = services
        .iter()
        .filter(|service| !service.disabled)
        .map(|service| service.name.as_str())
        .collect::<Vec<_>>();

    if available.is_empty() {
        "all declared compose services are disabled; pass `gat dx --service <name>` or set `git config gat.dockerService <name>`".to_string()
    } else {
        format!(
            "could not choose a docker service automatically; available services: {}. Pass `gat dx --service <name>` or set `git config gat.dockerService <name>`",
            available.join(", ")
        )
    }
}

fn exit_status_ok(status: ExitStatus, command: &str) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    Err(GatError::Io(format!(
        "{command} exited with status {}",
        render_status(status)
    )))
}

fn render_status(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn command_exists(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(command).is_file())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn service_header_accepts_two_space_indent() {
        assert_eq!(parse_service_header("  web:"), Some("web".to_string()));
        assert_eq!(parse_service_header("  dp1:"), Some("dp1".to_string()));
        assert_eq!(
            parse_service_header("  config-generator:"),
            Some("config-generator".to_string())
        );
    }

    #[test]
    fn service_header_rejects_non_service_lines() {
        // Four-space indent is a nested key, not a service header.
        assert_eq!(parse_service_header("    image: x"), None);
        // Root-level keys are not services.
        assert_eq!(parse_service_header("services:"), None);
        // Lines without a trailing colon are values, not headers.
        assert_eq!(parse_service_header("  image: nginx"), None);
        // Empty name.
        assert_eq!(parse_service_header("  :"), None);
    }

    /// Writes `content` to a unique temp compose file and returns its path.
    ///
    /// The `tag` keeps parallel tests from sharing (and clobbering) a path.
    fn temp_compose(tag: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gat-docker-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("docker-compose.yml");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn yaml_parser_lists_services_and_disabled() {
        let path = temp_compose(
            "yaml",
            "services:\n  base:\n    image: x\n    profiles: [\"_disabled\"]\n  dp1:\n    image: y\n",
        );
        let services = parse_declared_services_yaml(&path).unwrap();
        let base = services.iter().find(|s| s.name == "base").unwrap();
        let dp1 = services.iter().find(|s| s.name == "dp1").unwrap();
        assert!(base.disabled);
        assert!(!dp1.disabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn simple_parser_lists_services() {
        let path = temp_compose(
            "simple",
            "services:\n  alpha:\n    image: x\n  beta:\n    image: y\n",
        );
        let services = parse_declared_services_simple(&path).unwrap();
        let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn yes_no_renders_booleans() {
        assert_eq!(yes_no(true), "yes");
        assert_eq!(yes_no(false), "no");
    }
}
