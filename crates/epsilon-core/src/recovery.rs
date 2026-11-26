//! Crash-recovery autosaves.
//!
//! Autosaves are ordinary [`EpsilonFile`] documents written to a dedicated
//! directory under the application data folder. Each save gets a sortable
//! timestamp/sequence name, and only the newest [`RECOVERY_KEEP`] files are
//! retained. On launch the newest valid file can be offered to the user; a
//! normal save or an explicit discard removes the recovery files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::format::{EpsilonFile, FILE_EXTENSION, FileError};

/// Filenames created by the recovery store start with this marker.
pub const RECOVERY_PREFIX: &str = "autosave-";
/// Number of autosave files retained after a successful write.
pub const RECOVERY_KEEP: usize = 5;
/// App-state extras key holding the source document path, when there is one.
pub const RECOVERY_SOURCE_KEY: &str = "recovery_source_path";

static SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Sortable autosave filename: fixed-width timestamp then a per-process
/// sequence, so names sort chronologically even within one millisecond.
pub fn recovery_file_name(timestamp_millis: u128, sequence: u64) -> String {
    format!("{RECOVERY_PREFIX}{timestamp_millis:013}-{sequence:04}.{FILE_EXTENSION}")
}

/// Whether `name` is an autosave file this module owns.
pub fn is_recovery_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(RECOVERY_PREFIX) else {
        return false;
    };
    let Some(stem) = rest.strip_suffix(&format!(".{FILE_EXTENSION}")) else {
        return false;
    };
    let Some((timestamp, sequence)) = stem.split_once('-') else {
        return false;
    };
    timestamp.len() == 13
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.len() == 4
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

/// Oldest recovery filenames beyond `keep`, in the order they should be
/// removed. Non-recovery names are ignored.
pub fn files_to_prune(names: &[String], keep: usize) -> Vec<String> {
    let mut recovery: Vec<String> = names
        .iter()
        .filter(|name| is_recovery_file(name))
        .cloned()
        .collect();
    recovery.sort();
    if recovery.len() <= keep {
        return Vec::new();
    }
    recovery.drain(..recovery.len() - keep).collect()
}

/// A directory of rotating crash-recovery files.
#[derive(Debug, Clone)]
pub struct RecoveryStore {
    dir: PathBuf,
    keep: usize,
}

impl RecoveryStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            keep: RECOVERY_KEEP,
        }
    }

    /// Test/support constructor with an explicit retention count.
    pub fn with_keep(dir: impl Into<PathBuf>, keep: usize) -> Self {
        Self {
            dir: dir.into(),
            keep,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes a new autosave and prunes older ones. Returns the new path.
    pub fn save(&self, file: &EpsilonFile) -> Result<PathBuf, FileError> {
        fs::create_dir_all(&self.dir).map_err(FileError::Io)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                FileError::Io(std::io::Error::other(format!(
                    "system clock is before the Unix epoch: {error}"
                )))
            })?
            .as_millis();
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = self.dir.join(recovery_file_name(timestamp, sequence));
        file.save_to_path(&path)?;
        self.prune()?;
        Ok(path)
    }

    /// Newest recovery path, if one exists.
    pub fn latest_path(&self) -> Result<Option<PathBuf>, FileError> {
        Ok(self.names()?.last().map(|name| self.dir.join(name)))
    }

    /// Newest valid recovery document. Corrupt files are skipped so an old
    /// valid autosave can still be recovered.
    pub fn load_latest(&self) -> Result<Option<(PathBuf, EpsilonFile)>, FileError> {
        let mut names = self.names()?;
        names.sort();
        for name in names.into_iter().rev() {
            let path = self.dir.join(&name);
            match EpsilonFile::load_from_path(&path) {
                Ok(file) => return Ok(Some((path, file))),
                Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {}
            }
        }
        Ok(None)
    }

    /// Removes every autosave file. Missing directories are fine.
    pub fn discard_all(&self) -> Result<(), FileError> {
        if !self.dir.exists() {
            return Ok(());
        }
        for name in self.names()? {
            fs::remove_file(self.dir.join(name)).map_err(FileError::Io)?;
        }
        Ok(())
    }

    fn prune(&self) -> Result<(), FileError> {
        let names = self.names()?;
        for name in files_to_prune(&names, self.keep) {
            fs::remove_file(self.dir.join(name)).map_err(FileError::Io)?;
        }
        Ok(())
    }

    fn names(&self) -> Result<Vec<String>, FileError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&self.dir).map_err(FileError::Io)?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(FileError::Io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_recovery_file(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Element, ElementKind};
    use std::collections::BTreeMap;

    fn test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "epsilon-recovery-{}-{}-{label}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn file_with_rectangle(x: f64) -> EpsilonFile {
        let mut document = Document::new();
        document.add(Element::new(ElementKind::Rectangle, x, 0.0, 10.0, 10.0));
        EpsilonFile::new(document)
    }

    #[test]
    fn names_are_sortable_and_only_autosaves_are_recognised() {
        let first = recovery_file_name(1_700_000_000_000, 1);
        let second = recovery_file_name(1_700_000_000_001, 2);
        assert_eq!(first, "autosave-1700000000000-0001.epsilon");
        assert!(is_recovery_file(&first));
        assert!(second > first);
        assert!(!is_recovery_file("scene.epsilon"));
        assert!(!is_recovery_file("autosave-1-1.epsilon"));
        assert!(!is_recovery_file("autosave-1700000000000-0001.json"));
    }

    #[test]
    fn retention_keeps_the_newest_files() {
        let names: Vec<String> = (0..5)
            .map(|index| recovery_file_name(1_700_000_000_000 + index, index as u64))
            .chain(["scene.epsilon".to_owned()])
            .collect();
        let prune = files_to_prune(&names, 2);
        assert_eq!(prune, names[..3].to_vec());
        assert!(files_to_prune(&names, 5).is_empty());
        assert!(files_to_prune(&names, 10).is_empty());
    }

    #[test]
    fn store_rotates_retained_autosaves_and_loads_the_newest() {
        let dir = test_dir("store");
        let store = RecoveryStore::with_keep(&dir, 2);
        let mut last = None;
        for index in 0..4 {
            let file = file_with_rectangle(index as f64);
            store.save(&file).expect("save autosave");
            last = Some(file);
        }
        let names = store.names().expect("list autosaves");
        assert_eq!(names.len(), 2, "retention keeps exactly two files");

        let (_, restored) = store
            .load_latest()
            .expect("load latest")
            .expect("a recovery file");
        assert_eq!(Some(restored), last);

        std::fs::write(store.dir().join("unrelated.txt"), "keep me").expect("unrelated file");
        store.discard_all().expect("discard autosaves");
        assert!(store.names().expect("list autosaves").is_empty());
        assert!(store.dir().join("unrelated.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_newest_autosave_falls_back_to_an_older_valid_one() {
        let dir = test_dir("fallback");
        let store = RecoveryStore::with_keep(&dir, 5);
        let older = file_with_rectangle(1.0);
        store.save(&older).expect("save older");
        std::fs::write(
            dir.join(recovery_file_name(9_999_999_999_999, 1)),
            "not json",
        )
        .expect("write corrupt newer");

        let (_, restored) = store
            .load_latest()
            .expect("load latest")
            .expect("fallback file");
        assert_eq!(restored, older);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn source_path_metadata_is_plain_extras_data() {
        let mut file = file_with_rectangle(1.0);
        file.app_state.extras.insert(
            RECOVERY_SOURCE_KEY.to_owned(),
            serde_json::json!("/tmp/a.epsilon"),
        );
        let value: BTreeMap<String, serde_json::Value> = file.app_state.extras.clone();
        assert_eq!(value[RECOVERY_SOURCE_KEY], "/tmp/a.epsilon");
    }
}
