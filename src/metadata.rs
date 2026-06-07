//! Worktree metadata tracking for age and usage statistics.

use crate::error::{GatError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Metadata for a single worktree.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorktreeMetadata {
    /// When the worktree was created (Unix timestamp)
    pub created_at: u64,

    /// When the worktree was last accessed by gat (Unix timestamp)
    pub last_accessed_at: u64,

    /// Worktree path
    pub path: String,

    /// Branch name
    pub branch: String,

    /// Optional free-form description of what the worktree is for.
    ///
    /// Defaulted so metadata files written before descriptions existed still
    /// deserialize cleanly.
    #[serde(default)]
    pub description: String,
}

/// Global metadata store for all worktrees.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MetadataStore {
    /// Map of worktree path to metadata
    pub worktrees: HashMap<String, WorktreeMetadata>,
}

impl MetadataStore {
    /// Loads metadata from the repository's .git directory.
    pub fn load(repo_root: &Path) -> Result<Self> {
        let metadata_file = Self::metadata_path(repo_root)?;

        if !metadata_file.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&metadata_file)
            .map_err(|e| GatError::Io(format!("failed to read metadata: {e}")))?;

        let store: Self = serde_json::from_str(&contents)
            .map_err(|e| GatError::Io(format!("invalid metadata JSON: {e}")))?;

        Ok(store)
    }

    /// Saves metadata to the repository's .git directory.
    ///
    /// Writes to a temporary file in the same directory and atomically renames
    /// it into place. This prevents concurrent readers from observing a
    /// half-written file and avoids corruption if the process is interrupted
    /// mid-write.
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        let metadata_file = Self::metadata_path(repo_root)?;

        // Ensure parent directory exists
        if let Some(parent) = metadata_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| GatError::Io(format!("failed to create metadata dir: {e}")))?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| GatError::Io(format!("failed to serialize metadata: {e}")))?;

        // Write to a unique temp file, then atomically rename over the target.
        let tmp = metadata_file.with_extension(format!("json.tmp.{}", std::process::id()));
        fs::write(&tmp, json.as_bytes())
            .map_err(|e| GatError::Io(format!("failed to write metadata temp file: {e}")))?;
        fs::rename(&tmp, &metadata_file).map_err(|e| {
            // Best-effort cleanup of the temp file on failure.
            let _ = fs::remove_file(&tmp);
            GatError::Io(format!("failed to commit metadata file: {e}"))
        })?;

        Ok(())
    }

    /// Loads metadata, applies a mutation, and saves it back atomically.
    ///
    /// Centralizing the load-modify-save cycle keeps the window between read and
    /// write as small as possible and gives every caller the same persistence
    /// semantics.
    pub fn update<F>(repo_root: &Path, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut Self),
    {
        let mut store = Self::load(repo_root)?;
        mutate(&mut store);
        store.save(repo_root)
    }

    /// Returns the path to the metadata file.
    fn metadata_path(repo_root: &Path) -> Result<PathBuf> {
        let git_dir = repo_root.join(".git");
        if !git_dir.exists() {
            return Err(GatError::NotFound("not a git repository".into()));
        }

        Ok(git_dir.join("gat-metadata.json"))
    }

    /// Records that a worktree was created.
    ///
    /// Preserves any existing description when an entry for `path` already
    /// exists, so re-tracking a worktree never silently discards its label.
    pub fn track_creation(&mut self, path: &str, branch: &str) {
        let now = current_timestamp();
        let description = self
            .worktrees
            .get(path)
            .map(|m| m.description.clone())
            .unwrap_or_default();

        self.worktrees.insert(
            path.to_string(),
            WorktreeMetadata {
                created_at: now,
                last_accessed_at: now,
                path: path.to_string(),
                branch: branch.to_string(),
                description,
            },
        );
    }

    /// Sets (or clears) the description for a worktree.
    ///
    /// If no metadata entry exists yet for `path`, one is created so a
    /// description can be attached to worktrees made before tracking existed.
    /// Passing an empty string clears the description.
    pub fn set_description(&mut self, path: &str, branch: &str, description: &str) {
        let now = current_timestamp();
        let entry = self
            .worktrees
            .entry(path.to_string())
            .or_insert_with(|| WorktreeMetadata {
                created_at: now,
                last_accessed_at: now,
                path: path.to_string(),
                branch: branch.to_string(),
                description: String::new(),
            });
        entry.description = description.trim().to_string();
    }

    /// Returns the description for a worktree, if any non-empty one is set.
    pub fn description(&self, path: &str) -> Option<&str> {
        self.worktrees
            .get(path)
            .map(|m| m.description.as_str())
            .filter(|d| !d.is_empty())
    }

    /// Records that a worktree was accessed.
    pub fn track_access(&mut self, path: &str) {
        if let Some(metadata) = self.worktrees.get_mut(path) {
            metadata.last_accessed_at = current_timestamp();
        }
    }

    /// Removes metadata for a worktree.
    pub fn remove(&mut self, path: &str) {
        self.worktrees.remove(path);
    }

    /// Drops metadata entries whose `path` is not in `live_paths`.
    ///
    /// This bounds the metadata file size over time by discarding entries for
    /// worktrees Git no longer knows about (for example, removed outside of
    /// `gat`). Returns the number of entries removed.
    pub fn prune_missing(&mut self, live_paths: &std::collections::HashSet<String>) -> usize {
        let before = self.worktrees.len();
        self.worktrees.retain(|path, _| live_paths.contains(path));
        before - self.worktrees.len()
    }

    /// Returns worktrees not accessed within the specified number of days.
    pub fn stale_worktrees(&self, days: u64) -> Vec<&WorktreeMetadata> {
        let now = current_timestamp();
        let window = days.saturating_mul(24 * 60 * 60);
        let threshold = now.saturating_sub(window);

        self.worktrees
            .values()
            .filter(|m| m.last_accessed_at < threshold)
            .collect()
    }

    /// Returns days since last access for a tracked worktree.
    ///
    /// Falls back to the filesystem modification time when the worktree has no
    /// tracked entry, so age reporting works for worktrees created before
    /// tracking was enabled.
    pub fn days_since_access(&self, path: &str) -> Option<u64> {
        let now = current_timestamp();
        if let Some(m) = self.worktrees.get(path) {
            return Some(now.saturating_sub(m.last_accessed_at) / (24 * 60 * 60));
        }
        mtime_seconds(Path::new(path)).map(|mtime| now.saturating_sub(mtime) / (24 * 60 * 60))
    }
}

/// Returns current Unix timestamp.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns a path's modification time as seconds since the Unix epoch.
fn mtime_seconds(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|dur| dur.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_creation() {
        let mut store = MetadataStore::default();
        store.track_creation("/path/to/worktree", "feature-branch");

        assert_eq!(store.worktrees.len(), 1);
        assert!(store.worktrees.contains_key("/path/to/worktree"));
    }

    #[test]
    fn test_track_access() {
        let mut store = MetadataStore::default();
        store.track_creation("/path/to/worktree", "feature-branch");

        let initial_access = store
            .worktrees
            .get("/path/to/worktree")
            .unwrap()
            .last_accessed_at;

        // Simulate time passing (in real code, this would be actual time)
        store.track_access("/path/to/worktree");

        let updated_access = store
            .worktrees
            .get("/path/to/worktree")
            .unwrap()
            .last_accessed_at;

        assert!(updated_access >= initial_access);
    }

    #[test]
    fn test_remove() {
        let mut store = MetadataStore::default();
        store.track_creation("/path/to/worktree", "feature-branch");
        store.remove("/path/to/worktree");

        assert_eq!(store.worktrees.len(), 0);
    }

    #[test]
    fn test_stale_worktrees() {
        let mut store = MetadataStore::default();

        // Create worktree with old timestamp
        let metadata = WorktreeMetadata {
            created_at: current_timestamp() - (40 * 24 * 60 * 60), // 40 days ago
            last_accessed_at: current_timestamp() - (40 * 24 * 60 * 60),
            path: "/path/to/old".to_string(),
            branch: "old-branch".to_string(),
            description: String::new(),
        };
        store.worktrees.insert("/path/to/old".to_string(), metadata);

        // Create recent worktree
        store.track_creation("/path/to/new", "new-branch");

        let stale = store.stale_worktrees(30);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].path, "/path/to/old");
    }

    #[test]
    fn test_set_and_get_description() {
        let mut store = MetadataStore::default();
        store.track_creation("/wt", "feat");
        assert_eq!(store.description("/wt"), None);

        store.set_description("/wt", "feat", "  refactor auth flow  ");
        assert_eq!(store.description("/wt"), Some("refactor auth flow"));

        // Clearing with an empty string removes the description.
        store.set_description("/wt", "feat", "");
        assert_eq!(store.description("/wt"), None);
    }

    #[test]
    fn test_set_description_creates_entry_when_absent() {
        let mut store = MetadataStore::default();
        store.set_description("/wt", "feat", "investigate flake");
        assert_eq!(store.description("/wt"), Some("investigate flake"));
    }

    #[test]
    fn test_track_creation_preserves_existing_description() {
        let mut store = MetadataStore::default();
        store.set_description("/wt", "feat", "keep me");
        // Re-tracking the same path (e.g. reuse) must not wipe the description.
        store.track_creation("/wt", "feat");
        assert_eq!(store.description("/wt"), Some("keep me"));
    }

    #[test]
    fn test_prune_missing_drops_orphans() {
        let mut store = MetadataStore::default();
        store.track_creation("/path/live", "live");
        store.track_creation("/path/gone", "gone");

        let mut live = std::collections::HashSet::new();
        live.insert("/path/live".to_string());

        let removed = store.prune_missing(&live);
        assert_eq!(removed, 1);
        assert!(store.worktrees.contains_key("/path/live"));
        assert!(!store.worktrees.contains_key("/path/gone"));
    }

    #[test]
    fn test_stale_window_does_not_underflow() {
        // A huge day count must not panic via overflow; it should simply
        // produce a zero threshold and match nothing newer than the epoch.
        let mut store = MetadataStore::default();
        store.track_creation("/recent", "recent");
        let stale = store.stale_worktrees(u64::MAX);
        assert!(stale.is_empty());
    }

    #[test]
    fn test_days_since_access_untracked_uses_mtime_fallback() {
        // A freshly created temp file is ~0 days old and is not tracked, so the
        // mtime fallback should report a small, non-None value.
        let dir = std::env::temp_dir().join(format!("gat-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = MetadataStore::default();
        let days = store.days_since_access(&dir.to_string_lossy());
        assert_eq!(days, Some(0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_load_roundtrip_atomic() {
        let dir = std::env::temp_dir().join(format!("gat-meta-rt-{}", std::process::id()));
        let git_dir = dir.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();

        MetadataStore::update(&dir, |m| m.track_creation("/wt/a", "branch-a")).unwrap();
        let loaded = MetadataStore::load(&dir).unwrap();
        assert!(loaded.worktrees.contains_key("/wt/a"));

        // No stray temp files should be left behind after an atomic save.
        let leftovers: Vec<_> = std::fs::read_dir(&git_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
