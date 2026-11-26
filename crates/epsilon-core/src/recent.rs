//! Bounded recent-document paths, persisted with app settings.
//!
//! Paths are ordered most-recent first, deduplicated by exact path, and capped
//! at [`RECENT_LIMIT`]. Missing files can be pruned without touching the
//! document model; opening a stale entry is an app-level decision.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Maximum number of recent paths retained.
pub const RECENT_LIMIT: usize = 8;

/// Most-recently-used document paths.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Moves `path` to the front, removing any earlier entry with the same
    /// path, and enforces the cap.
    pub fn record(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.paths.retain(|current| current != &path);
        self.paths.insert(0, path);
        self.paths.truncate(RECENT_LIMIT);
    }

    /// Removes one path, returning whether it was present.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.paths.len();
        self.paths.retain(|current| current != path);
        self.paths.len() != before
    }

    /// Removes entries that are not regular files. Returns the number removed.
    pub fn prune_missing(&mut self) -> usize {
        let before = self.paths.len();
        self.paths.retain(|path| path.is_file());
        before - self.paths.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("epsilon-recent-{}-{label}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn record_deduplicates_moves_to_front_and_caps_the_list() {
        let mut recent = RecentFiles::new();
        for index in 0..(RECENT_LIMIT + 3) {
            recent.record(format!("/tmp/scene-{index}.epsilon"));
        }
        assert_eq!(recent.len(), RECENT_LIMIT);
        assert_eq!(
            recent.paths()[0],
            PathBuf::from(format!("/tmp/scene-{}.epsilon", RECENT_LIMIT + 2))
        );

        recent.record("/tmp/scene-9.epsilon");
        assert_eq!(recent.paths()[0], PathBuf::from("/tmp/scene-9.epsilon"));
        assert_eq!(
            recent
                .paths()
                .iter()
                .filter(|path| *path == Path::new("/tmp/scene-9.epsilon"))
                .count(),
            1
        );
    }

    #[test]
    fn remove_and_prune_missing_files() {
        let dir = temp_dir("prune");
        let present = dir.join("present.epsilon");
        let missing = dir.join("missing.epsilon");
        std::fs::write(&present, "x").expect("write file");

        let mut recent = RecentFiles::new();
        recent.record(&present);
        recent.record(&missing);
        assert!(recent.remove(&missing));
        assert!(!recent.remove(&missing));
        assert_eq!(recent.paths(), std::slice::from_ref(&present));

        recent.record(&missing);
        assert_eq!(recent.prune_missing(), 1);
        assert_eq!(recent.paths(), &[present]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recent_paths_round_trip_through_serialization() {
        let mut recent = RecentFiles::new();
        recent.record("/tmp/a.epsilon");
        recent.record("/tmp/b.epsilon");
        let json = serde_json::to_string(&recent).expect("serialize recent");
        let restored: RecentFiles = serde_json::from_str(&json).expect("deserialize recent");
        assert_eq!(restored, recent);
        assert_eq!(
            serde_json::from_str::<RecentFiles>("[]").expect("empty list"),
            RecentFiles::new()
        );
    }
}
