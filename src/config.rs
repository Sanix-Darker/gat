//! Configuration management for `gat`.
//!
//! Supports git config, environment variables, and optional config files.

use crate::error::{GatError, Result};
use crate::git::Repo;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// A worktree setup template applied after `gat new` creates a worktree.
///
/// Templates make new worktrees immediately usable by copying ignored config
/// files, symlinking shared dependency directories, and running setup commands.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Template {
    /// Files to copy from the primary worktree into the new worktree.
    ///
    /// Paths are relative to each worktree root. Missing sources are skipped
    /// with a warning rather than failing the whole operation.
    #[serde(default)]
    pub copy: Vec<String>,

    /// Directories to symlink from the primary worktree into the new worktree.
    ///
    /// Useful for large shared caches such as `node_modules` or `target`.
    #[serde(default)]
    pub symlink: Vec<String>,

    /// Shell commands to run in the new worktree after copy/symlink steps.
    #[serde(default)]
    pub run: Vec<String>,
}

/// Named tmux layout presets.
///
/// A preset is a convenient shorthand for the underlying pane geometry
/// (`left_width`, `bottom_height`, `focus_left`). Explicit geometry settings
/// override the preset baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutPreset {
    /// Balanced 3-pane layout: AI left (55%), editor and shell right.
    Classic,
    /// Large AI pane (70%) for AI-driven work.
    AiFocus,
    /// Small AI pane (35%) with the editor in focus.
    EditorFocus,
    /// Even 50/50 split between AI and editor.
    Wide,
}

impl LayoutPreset {
    /// Resolves a preset from its name, accepting common spellings.
    ///
    /// Returns `None` for unknown names so callers can warn and fall back.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().replace('_', "-").as_str() {
            "classic" | "default" => Some(Self::Classic),
            "ai-focus" | "ai" => Some(Self::AiFocus),
            "editor-focus" | "editor" => Some(Self::EditorFocus),
            "wide" | "side-by-side" | "5050" | "50-50" => Some(Self::Wide),
            _ => None,
        }
    }

    /// Returns the `(left_width, bottom_height, focus_left)` geometry.
    fn geometry(self) -> (u8, u8, bool) {
        match self {
            Self::Classic => (55, 35, true),
            Self::AiFocus => (70, 40, true),
            Self::EditorFocus => (35, 25, false),
            Self::Wide => (50, 50, true),
        }
    }

    /// Applies the preset geometry to a [`TmuxLayout`] as a baseline.
    fn apply(self, layout: &mut TmuxLayout) {
        let (left_width, bottom_height, focus_left) = self.geometry();
        layout.left_width = left_width;
        layout.bottom_height = bottom_height;
        layout.focus_left = focus_left;
    }
}

/// Tmux layout configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TmuxLayout {
    /// Left pane width percentage (0-100).
    #[serde(default = "default_left_width")]
    pub left_width: u8,

    /// Right bottom pane height percentage (0-100).
    #[serde(default = "default_bottom_height")]
    pub bottom_height: u8,

    /// Shell path for tmux panes.
    #[serde(default = "default_shell")]
    pub shell: String,

    /// Command for left AI pane.
    #[serde(default = "default_codex_cmd")]
    pub codex_cmd: String,

    /// Command for right editor pane.
    #[serde(default = "default_editor_cmd")]
    pub editor_cmd: String,

    /// Whether to focus left pane on creation.
    #[serde(default = "default_focus_left")]
    pub focus_left: bool,
}

fn default_left_width() -> u8 {
    55
}
fn default_bottom_height() -> u8 {
    35
}
fn default_shell() -> String {
    env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}
fn default_codex_cmd() -> String {
    "codex".to_string()
}
fn default_editor_cmd() -> String {
    "nvim".to_string()
}
fn default_focus_left() -> bool {
    true
}

impl Default for TmuxLayout {
    fn default() -> Self {
        Self {
            left_width: default_left_width(),
            bottom_height: default_bottom_height(),
            shell: default_shell(),
            codex_cmd: default_codex_cmd(),
            editor_cmd: default_editor_cmd(),
            focus_left: default_focus_left(),
        }
    }
}

/// Global configuration for gat.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GatConfig {
    /// Default ticket prefix.
    #[serde(default)]
    pub ticket_prefix: Option<String>,

    /// Tmux layout configuration.
    #[serde(default)]
    pub tmux: TmuxLayout,

    /// Docker compose directory.
    #[serde(default)]
    pub docker_compose_dir: Option<String>,

    /// Docker worktree mount path.
    #[serde(default)]
    pub docker_worktree_mount: Option<String>,

    /// Docker default service.
    #[serde(default)]
    pub docker_service: Option<String>,

    /// Enable verbose logging.
    #[serde(default)]
    pub verbose: bool,

    /// Named worktree setup templates, keyed by template name.
    #[serde(default)]
    pub templates: std::collections::HashMap<String, Template>,
}

impl GatConfig {
    /// Loads configuration from file, git config, and environment.
    ///
    /// Priority (highest to lowest):
    /// 1. Environment variables (GAT_*)
    /// 2. Repository git config (gat.*)
    /// 3. Global git config
    /// 4. Config file (~/.config/gat/config.toml)
    /// 5. Defaults
    pub fn load(repo: Option<&Repo>) -> Result<Self> {
        let mut config = Self::load_from_file()?;

        if let Some(repo) = repo {
            config.merge_from_git_config(repo)?;
        }

        config.merge_from_env();

        Ok(config)
    }

    /// Loads only the on-disk config file (no git/env overlay).
    ///
    /// Used by `gat config set`, which must persist a change to the file
    /// without folding in git/env values that are not stored there.
    pub fn load_from_file_public() -> Result<Self> {
        Self::load_from_file()
    }

    /// Loads configuration from file if it exists.
    fn load_from_file() -> Result<Self> {
        let config_path = config_file_path()?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&config_path)
            .map_err(|e| GatError::Io(format!("failed to read config file: {e}")))?;

        let config: Self = toml::from_str(&contents)
            .map_err(|e| GatError::Io(format!("invalid config file: {e}")))?;

        Ok(config)
    }

    /// Merges configuration from git config.
    fn merge_from_git_config(&mut self, repo: &Repo) -> Result<()> {
        use crate::git;

        if let Some(prefix) = git::config_get(repo, "gat.ticketPrefix")? {
            self.ticket_prefix = Some(prefix);
        }

        if let Some(compose_dir) = git::config_get(repo, "gat.dockerComposeDir")? {
            self.docker_compose_dir = Some(compose_dir);
        }

        if let Some(mount) = git::config_get(repo, "gat.dockerWorktreeMount")? {
            self.docker_worktree_mount = Some(mount);
        }

        if let Some(service) = git::config_get(repo, "gat.dockerService")? {
            self.docker_service = Some(service);
        }

        // Apply a named layout preset before explicit geometry so explicit
        // width/height still win.
        if let Some(layout) = git::config_get(repo, "gat.tmuxLayout")? {
            self.apply_layout_preset(&layout);
        }

        if let Some(shell) = git::config_get(repo, "gat.tmuxShell")? {
            self.tmux.shell = shell;
        }

        if let Some(codex) = git::config_get(repo, "gat.tmuxCodexCmd")? {
            self.tmux.codex_cmd = codex;
        }

        if let Some(editor) = git::config_get(repo, "gat.tmuxEditorCmd")? {
            self.tmux.editor_cmd = editor;
        }

        if let Some(width) = git::config_get(repo, "gat.tmuxLeftWidth")? {
            if let Ok(w) = width.parse::<u8>() {
                if w <= 100 {
                    self.tmux.left_width = w;
                }
            }
        }

        if let Some(height) = git::config_get(repo, "gat.tmuxBottomHeight")? {
            if let Ok(h) = height.parse::<u8>() {
                if h <= 100 {
                    self.tmux.bottom_height = h;
                }
            }
        }

        Ok(())
    }

    /// Applies a named layout preset, logging a warning for unknown names.
    pub fn apply_layout_preset(&mut self, name: &str) {
        match LayoutPreset::from_name(name) {
            Some(preset) => preset.apply(&mut self.tmux),
            None => log::warn!(
                "unknown tmux layout '{name}'; expected classic, ai-focus, editor-focus, or wide"
            ),
        }
    }

    /// Serializes the configuration to the file's TOML format.
    ///
    /// Only the keys understood by the file parser are emitted, so a written
    /// file round-trips cleanly back through [`Self::load_from_file`].
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# gat configuration\n");
        if let Some(prefix) = &self.ticket_prefix {
            out.push_str(&format!("ticket_prefix = \"{prefix}\"\n"));
        }
        if let Some(dir) = &self.docker_compose_dir {
            out.push_str(&format!("docker_compose_dir = \"{dir}\"\n"));
        }
        if let Some(mount) = &self.docker_worktree_mount {
            out.push_str(&format!("docker_worktree_mount = \"{mount}\"\n"));
        }
        if let Some(service) = &self.docker_service {
            out.push_str(&format!("docker_service = \"{service}\"\n"));
        }
        out.push_str(&format!("verbose = {}\n", self.verbose));
        out.push_str("\n[tmux]\n");
        out.push_str(&format!("left_width = {}\n", self.tmux.left_width));
        out.push_str(&format!("bottom_height = {}\n", self.tmux.bottom_height));
        out.push_str(&format!("shell = \"{}\"\n", self.tmux.shell));
        out.push_str(&format!("codex_cmd = \"{}\"\n", self.tmux.codex_cmd));
        out.push_str(&format!("editor_cmd = \"{}\"\n", self.tmux.editor_cmd));
        out.push_str(&format!("focus_left = {}\n", self.tmux.focus_left));
        // Templates, one section each.
        let mut names: Vec<&String> = self.templates.keys().collect();
        names.sort();
        for name in names {
            let t = &self.templates[name];
            out.push_str(&format!("\n[template.{name}]\n"));
            if !t.copy.is_empty() {
                out.push_str(&format!("copy = {}\n", toml_string_array(&t.copy)));
            }
            if !t.symlink.is_empty() {
                out.push_str(&format!("symlink = {}\n", toml_string_array(&t.symlink)));
            }
            if !t.run.is_empty() {
                out.push_str(&format!("run = {}\n", toml_string_array(&t.run)));
            }
        }
        out
    }

    /// Writes the configuration to the config file, creating parent dirs.
    pub fn save(&self) -> Result<PathBuf> {
        let path = config_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| GatError::Io(format!("failed to create config dir: {e}")))?;
        }
        fs::write(&path, self.to_toml())
            .map_err(|e| GatError::Io(format!("failed to write config file: {e}")))?;
        Ok(path)
    }

    /// Returns the value of a dotted config key, or `None` if unset/unknown.
    ///
    /// Recognized keys: `ticket_prefix`, `docker_compose_dir`,
    /// `docker_worktree_mount`, `docker_service`, `verbose`, and the
    /// `tmux.<field>` keys.
    pub fn get_key(&self, key: &str) -> Option<String> {
        match key {
            "ticket_prefix" => self.ticket_prefix.clone(),
            "docker_compose_dir" => self.docker_compose_dir.clone(),
            "docker_worktree_mount" => self.docker_worktree_mount.clone(),
            "docker_service" => self.docker_service.clone(),
            "verbose" => Some(self.verbose.to_string()),
            "tmux.left_width" => Some(self.tmux.left_width.to_string()),
            "tmux.bottom_height" => Some(self.tmux.bottom_height.to_string()),
            "tmux.shell" => Some(self.tmux.shell.clone()),
            "tmux.codex_cmd" => Some(self.tmux.codex_cmd.clone()),
            "tmux.editor_cmd" => Some(self.tmux.editor_cmd.clone()),
            "tmux.focus_left" => Some(self.tmux.focus_left.to_string()),
            "tmux.layout" => None, // a preset is write-only sugar; no stored value
            _ => None,
        }
    }

    /// Sets a dotted config key, validating the value.
    ///
    /// Returns an error for unknown keys or values that fail validation (for
    /// example a percentage outside 0-100).
    pub fn set_key(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "ticket_prefix" => self.ticket_prefix = non_empty_owned(value),
            "docker_compose_dir" => self.docker_compose_dir = non_empty_owned(value),
            "docker_worktree_mount" => self.docker_worktree_mount = non_empty_owned(value),
            "docker_service" => self.docker_service = non_empty_owned(value),
            "verbose" => self.verbose = parse_bool(value)?,
            "tmux.layout" => self.apply_layout_preset(value),
            "tmux.left_width" => self.tmux.left_width = parse_percent(value)?,
            "tmux.bottom_height" => self.tmux.bottom_height = parse_percent(value)?,
            "tmux.shell" => self.tmux.shell = value.to_string(),
            "tmux.codex_cmd" => self.tmux.codex_cmd = value.to_string(),
            "tmux.editor_cmd" => self.tmux.editor_cmd = value.to_string(),
            "tmux.focus_left" => self.tmux.focus_left = parse_bool(value)?,
            _ => {
                return Err(GatError::Usage(format!(
                    "unknown config key '{key}'; run `gat config list` to see valid keys"
                )))
            }
        }
        Ok(())
    }

    /// Returns all recognized keys paired with their current values.
    pub fn entries(&self) -> Vec<(&'static str, String)> {
        const KEYS: &[&str] = &[
            "ticket_prefix",
            "docker_compose_dir",
            "docker_worktree_mount",
            "docker_service",
            "verbose",
            "tmux.left_width",
            "tmux.bottom_height",
            "tmux.shell",
            "tmux.codex_cmd",
            "tmux.editor_cmd",
            "tmux.focus_left",
        ];
        KEYS.iter()
            .map(|k| (*k, self.get_key(k).unwrap_or_default()))
            .collect()
    }

    /// Merges configuration from environment variables.
    fn merge_from_env(&mut self) {
        if let Some(prefix) = non_empty_env("GAT_TICKET_PREFIX") {
            self.ticket_prefix = Some(prefix);
        }

        if let Some(dir) = non_empty_env("GAT_DOCKER_COMPOSE_DIR") {
            self.docker_compose_dir = Some(dir);
        }

        if let Some(mount) = non_empty_env("GAT_DOCKER_WORKTREE_MOUNT") {
            self.docker_worktree_mount = Some(mount);
        }

        if let Some(service) = non_empty_env("GAT_DOCKER_SERVICE") {
            self.docker_service = Some(service);
        }

        // Layout preset before explicit geometry env vars.
        if let Some(layout) = non_empty_env("GAT_TMUX_LAYOUT") {
            self.apply_layout_preset(&layout);
        }

        if let Some(shell) = non_empty_env("GAT_TMUX_SHELL") {
            self.tmux.shell = shell;
        }

        if let Some(codex) = non_empty_env("GAT_TMUX_CODEX_CMD") {
            self.tmux.codex_cmd = codex;
        }

        if let Some(editor) = non_empty_env("GAT_TMUX_EDITOR_CMD") {
            self.tmux.editor_cmd = editor;
        }

        if let Some(width) = non_empty_env("GAT_TMUX_LEFT_WIDTH") {
            if let Ok(w) = width.parse::<u8>() {
                if w <= 100 {
                    self.tmux.left_width = w;
                }
            }
        }

        if let Some(height) = non_empty_env("GAT_TMUX_BOTTOM_HEIGHT") {
            if let Ok(h) = height.parse::<u8>() {
                if h <= 100 {
                    self.tmux.bottom_height = h;
                }
            }
        }

        if env::var("GAT_VERBOSE").is_ok() {
            self.verbose = true;
        }
    }
}

/// Returns the configuration file path.
pub fn config_file_path() -> Result<PathBuf> {
    gat_config_root().map(|root| root.join("config.toml"))
}

/// Returns the configuration root directory.
pub fn gat_config_root() -> Result<PathBuf> {
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

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

/// Renders a list of strings as a TOML array literal: `["a", "b"]`.
fn toml_string_array(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Returns the trimmed value as `Some`, or `None` when empty (to clear a key).
fn non_empty_owned(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parses a boolean config value, accepting common spellings.
fn parse_bool(value: &str) -> Result<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(GatError::Usage(format!(
            "expected a boolean (true/false), got '{value}'"
        ))),
    }
}

/// Parses a percentage config value in the range 0-100.
fn parse_percent(value: &str) -> Result<u8> {
    let parsed = value
        .trim()
        .parse::<u8>()
        .map_err(|_| GatError::Usage(format!("expected a number 0-100, got '{value}'")))?;
    if parsed > 100 {
        return Err(GatError::Usage(format!(
            "percentage must be 0-100, got {parsed}"
        )));
    }
    Ok(parsed)
}

// Temporary toml parsing (should use toml crate, but keeping dependencies minimal)
mod toml {
    use super::GatConfig;

    pub fn from_str(s: &str) -> Result<GatConfig, String> {
        let mut config = GatConfig::default();
        // Current section: None = root, Some("tmux"), or Some("template:<name>").
        let mut section: Option<String> = None;

        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let header = header.trim();
                if let Some(name) = header.strip_prefix("template.") {
                    let name = name.trim().trim_matches('"').to_string();
                    // Ensure the template exists so empty sections still register.
                    config.templates.entry(name.clone()).or_default();
                    section = Some(format!("template:{name}"));
                } else if header == "tmux" {
                    section = Some("tmux".to_string());
                } else {
                    section = None;
                }
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }

            let key = parts[0].trim();
            let raw_value = parts[1].trim();
            let value = raw_value.trim_matches('"');

            match section.as_deref() {
                Some("tmux") => match key {
                    "layout" => config.apply_layout_preset(value),
                    "left_width" => config.tmux.left_width = value.parse().unwrap_or(55),
                    "bottom_height" => config.tmux.bottom_height = value.parse().unwrap_or(35),
                    "shell" => config.tmux.shell = value.to_string(),
                    "codex_cmd" => config.tmux.codex_cmd = value.to_string(),
                    "editor_cmd" => config.tmux.editor_cmd = value.to_string(),
                    "focus_left" => config.tmux.focus_left = value == "true",
                    _ => {}
                },
                Some(other) if other.starts_with("template:") => {
                    let name = other.trim_start_matches("template:").to_string();
                    let template = config.templates.entry(name).or_default();
                    // List values use a simple TOML array form: ["a", "b"].
                    let items = parse_string_array(raw_value);
                    match key {
                        "copy" => template.copy = items,
                        "symlink" => template.symlink = items,
                        "run" => template.run = items,
                        _ => {}
                    }
                }
                _ => match key {
                    "ticket_prefix" => config.ticket_prefix = Some(value.to_string()),
                    "docker_compose_dir" => config.docker_compose_dir = Some(value.to_string()),
                    "docker_worktree_mount" => {
                        config.docker_worktree_mount = Some(value.to_string())
                    }
                    "docker_service" => config.docker_service = Some(value.to_string()),
                    "verbose" => config.verbose = value == "true",
                    _ => {}
                },
            }
        }

        Ok(config)
    }

    /// Parses a simple TOML string-array value: `["a", "b"]` or a bare string.
    ///
    /// This is intentionally small: it splits on commas and strips quotes and
    /// brackets, which covers the values gat templates use.
    fn parse_string_array(raw: &str) -> Vec<String> {
        let trimmed = raw.trim();
        let inner = trimmed
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .unwrap_or(trimmed);
        inner
            .split(',')
            .map(|item| item.trim().trim_matches('"').trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GatConfig::default();
        assert_eq!(config.tmux.left_width, 55);
        assert_eq!(config.tmux.bottom_height, 35);
        assert!(config.tmux.focus_left);
    }

    #[test]
    fn test_tmux_layout_percentages() {
        let layout = TmuxLayout::default();
        assert!(layout.left_width <= 100);
        assert!(layout.bottom_height <= 100);
    }

    #[test]
    fn test_layout_preset_from_name() {
        assert_eq!(
            LayoutPreset::from_name("classic"),
            Some(LayoutPreset::Classic)
        );
        assert_eq!(
            LayoutPreset::from_name("default"),
            Some(LayoutPreset::Classic)
        );
        assert_eq!(
            LayoutPreset::from_name("ai-focus"),
            Some(LayoutPreset::AiFocus)
        );
        assert_eq!(
            LayoutPreset::from_name("AI_FOCUS"),
            Some(LayoutPreset::AiFocus)
        );
        assert_eq!(
            LayoutPreset::from_name("editor-focus"),
            Some(LayoutPreset::EditorFocus)
        );
        assert_eq!(LayoutPreset::from_name("wide"), Some(LayoutPreset::Wide));
        assert_eq!(
            LayoutPreset::from_name("side-by-side"),
            Some(LayoutPreset::Wide)
        );
        assert_eq!(LayoutPreset::from_name("nonsense"), None);
    }

    #[test]
    fn test_apply_layout_preset_sets_geometry() {
        let mut config = GatConfig::default();
        config.apply_layout_preset("ai-focus");
        assert_eq!(config.tmux.left_width, 70);
        assert_eq!(config.tmux.bottom_height, 40);
        assert!(config.tmux.focus_left);

        config.apply_layout_preset("editor-focus");
        assert_eq!(config.tmux.left_width, 35);
        assert!(!config.tmux.focus_left);
    }

    #[test]
    fn test_unknown_preset_leaves_geometry_unchanged() {
        let mut config = GatConfig::default();
        let before = config.tmux.left_width;
        config.apply_layout_preset("does-not-exist");
        assert_eq!(config.tmux.left_width, before);
    }

    #[test]
    fn test_file_layout_preset_then_explicit_override() {
        // layout sets a baseline; an explicit left_width after it wins.
        let toml = "[tmux]\nlayout = \"ai-focus\"\nleft_width = 60\n";
        let config = super::toml::from_str(toml).unwrap();
        assert_eq!(config.tmux.left_width, 60);
        // bottom_height came from the preset since it was not overridden.
        assert_eq!(config.tmux.bottom_height, 40);
    }

    #[test]
    fn test_to_toml_roundtrips_through_parser() {
        let config = GatConfig {
            ticket_prefix: Some("ABC".to_string()),
            verbose: true,
            tmux: TmuxLayout {
                left_width: 62,
                codex_cmd: "aider".to_string(),
                ..TmuxLayout::default()
            },
            ..GatConfig::default()
        };

        let rendered = config.to_toml();
        let parsed = super::toml::from_str(&rendered).unwrap();

        assert_eq!(parsed.ticket_prefix.as_deref(), Some("ABC"));
        assert_eq!(parsed.tmux.left_width, 62);
        assert_eq!(parsed.tmux.codex_cmd, "aider");
        assert!(parsed.verbose);
    }

    #[test]
    fn test_get_and_set_key() {
        let mut config = GatConfig::default();
        config.set_key("ticket_prefix", "GAT").unwrap();
        assert_eq!(config.get_key("ticket_prefix").as_deref(), Some("GAT"));

        config.set_key("tmux.left_width", "70").unwrap();
        assert_eq!(config.get_key("tmux.left_width").as_deref(), Some("70"));

        config.set_key("tmux.focus_left", "false").unwrap();
        assert_eq!(config.get_key("tmux.focus_left").as_deref(), Some("false"));
    }

    #[test]
    fn test_set_key_validation() {
        let mut config = GatConfig::default();
        assert!(config.set_key("tmux.left_width", "150").is_err());
        assert!(config.set_key("tmux.left_width", "abc").is_err());
        assert!(config.set_key("verbose", "maybe").is_err());
        assert!(config.set_key("unknown.key", "x").is_err());
    }

    #[test]
    fn test_set_key_clears_with_empty_value() {
        let mut config = GatConfig::default();
        config.set_key("ticket_prefix", "GAT").unwrap();
        config.set_key("ticket_prefix", "").unwrap();
        assert_eq!(config.get_key("ticket_prefix"), None);
    }

    #[test]
    fn test_entries_lists_all_keys() {
        let config = GatConfig::default();
        let entries = config.entries();
        assert!(entries.iter().any(|(k, _)| *k == "tmux.left_width"));
        assert!(entries.iter().any(|(k, _)| *k == "ticket_prefix"));
        // Every entry key must round-trip through get_key.
        for (key, _) in &entries {
            assert!(config.get_key(key).is_some() || config.get_key(key).is_none());
        }
    }

    #[test]
    fn test_template_section_parsing() {
        let toml = r#"
[template.node]
copy = [".env.example", "config/local.json"]
symlink = ["node_modules"]
run = ["npm install"]
"#;
        let config = super::toml::from_str(toml).unwrap();
        let t = config.templates.get("node").expect("template node exists");
        assert_eq!(t.copy, vec![".env.example", "config/local.json"]);
        assert_eq!(t.symlink, vec!["node_modules"]);
        assert_eq!(t.run, vec!["npm install"]);
    }

    #[test]
    fn test_template_roundtrips_through_writer() {
        let mut config = GatConfig::default();
        config.templates.insert(
            "rust".to_string(),
            Template {
                copy: vec![".env".to_string()],
                symlink: vec!["target".to_string()],
                run: vec!["cargo fetch".to_string()],
            },
        );
        let rendered = config.to_toml();
        let parsed = super::toml::from_str(&rendered).unwrap();
        let t = parsed.templates.get("rust").expect("rust template");
        assert_eq!(t.copy, vec![".env"]);
        assert_eq!(t.symlink, vec!["target"]);
        assert_eq!(t.run, vec!["cargo fetch"]);
    }
}
