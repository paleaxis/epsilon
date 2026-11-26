//! Local, rotating diagnostic log.
//!
//! Epsilon is offline-first, so "logging" means a plain text file under the
//! app data directory and nothing else: no telemetry, no network, no uploads.
//! Writes are best-effort — an unwritable log must never take the editor down
//! or replace the error the user is actually looking at.
//!
//! The log is capped at [`MAX_LOG_BYTES`] and rotated into numbered siblings,
//! keeping [`ROTATED_FILES`] of them. Rotation is size-triggered on write, so a
//! long session cannot grow the file without bound.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// File name of the current log inside the log directory.
pub(crate) const LOG_FILE_NAME: &str = "epsilon.log";
/// Rotate once the current log reaches this size (1 MiB).
pub(crate) const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// Number of rotated siblings to keep (`epsilon.log.1` … `epsilon.log.3`).
pub(crate) const ROTATED_FILES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Appends records to `epsilon.log`, rotating it when it grows too large.
pub(crate) struct LogWriter {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
}

impl LogWriter {
    pub(crate) fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            max_bytes: MAX_LOG_BYTES,
            keep: ROTATED_FILES,
        }
    }

    #[cfg(test)]
    fn with_limits(mut self, max_bytes: u64, keep: usize) -> Self {
        self.max_bytes = max_bytes;
        self.keep = keep;
        self
    }

    fn path(&self) -> PathBuf {
        self.dir.join(LOG_FILE_NAME)
    }

    fn rotated_path(&self, index: usize) -> PathBuf {
        self.dir.join(format!("{LOG_FILE_NAME}.{index}"))
    }

    /// Writes one record. Failures are deliberately dropped: the caller is
    /// usually already reporting a user-facing error, and a full disk must not
    /// turn that into a panic.
    pub(crate) fn write(&self, level: Level, scope: &str, message: &str) {
        // `let _ =` is not used anywhere else; here the only alternative would
        // be panicking on an unwritable log, which is worse than losing a line.
        if let Err(error) = self.try_write(level, scope, message) {
            eprintln!("epsilon: could not write log: {error}");
        }
    }

    fn try_write(&self, level: Level, scope: &str, message: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        self.rotate_if_needed()?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        let line = format!(
            "{} [{}] {}: {}\n",
            timestamp(SystemTime::now()),
            level.label(),
            sanitize(scope),
            sanitize(message)
        );
        file.write_all(line.as_bytes())?;
        file.flush()
    }

    fn rotate_if_needed(&self) -> io::Result<()> {
        let size = match fs::metadata(self.path()) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if size < self.max_bytes {
            return Ok(());
        }
        self.rotate()
    }

    fn rotate(&self) -> io::Result<()> {
        if self.keep == 0 {
            return remove_if_present(&self.path());
        }
        remove_if_present(&self.rotated_path(self.keep))?;
        for index in (1..self.keep).rev() {
            rename_if_present(&self.rotated_path(index), &self.rotated_path(index + 1))?;
        }
        rename_if_present(&self.path(), &self.rotated_path(1))
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn rename_if_present(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Records must stay one per line so the file remains greppable.
fn sanitize(text: &str) -> String {
    text.replace("\r\n", "\\n").replace(['\n', '\r'], "\\n")
}

/// RFC 3339-like UTC timestamp without a date/time crate: the log needs a
/// readable clock, not a full calendar library.
fn timestamp(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format_epoch_seconds(seconds as i64)
}

fn format_epoch_seconds(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`, valid for the whole epoch range).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

static WRITER: OnceLock<LogWriter> = OnceLock::new();

/// Installs the process-wide log file. Repeated calls are ignored, so tests and
/// a future second window cannot fight over the destination.
pub(crate) fn init(dir: &Path) {
    if WRITER.set(LogWriter::new(dir)).is_err() {
        return;
    }
    info(
        "app",
        &format!("session started (Epsilon {})", env!("CARGO_PKG_VERSION")),
    );
}

pub(crate) fn error(scope: &str, message: &str) {
    if let Some(writer) = WRITER.get() {
        writer.write(Level::Error, scope, message);
    }
}

pub(crate) fn warn(scope: &str, message: &str) {
    if let Some(writer) = WRITER.get() {
        writer.write(Level::Warn, scope, message);
    }
}

pub(crate) fn info(scope: &str, message: &str) {
    if let Some(writer) = WRITER.get() {
        writer.write(Level::Info, scope, message);
    }
}

/// Logs panics (message and source location) before deferring to the default
/// hook, so a crash still produces a backtrace on stderr and a record on disk.
pub(crate) fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown location".to_owned());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        error("panic", &format!("panic at {location}: {payload}"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "epsilon-log-{}-{}-{label}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn timestamp_formats_known_epoch_values() {
        assert_eq!(format_epoch_seconds(0), "1970-01-01 00:00:00Z");
        assert_eq!(format_epoch_seconds(1_700_000_000), "2023-11-14 22:13:20Z");
        // Leap day, to exercise the civil-date conversion.
        assert_eq!(format_epoch_seconds(1_709_208_000), "2024-02-29 12:00:00Z");
    }

    #[test]
    fn newlines_in_messages_stay_on_one_record() {
        assert_eq!(sanitize("first\nsecond\r\nthird"), "first\\nsecond\\nthird");
    }

    #[test]
    fn writes_records_with_level_scope_and_timestamp() {
        let dir = test_dir("write");
        let writer = LogWriter::new(&dir);
        writer.write(Level::Info, "file", "Saved scene.epsilon");
        writer.write(Level::Error, "export", "Export failed: disk full");

        let text = fs::read_to_string(dir.join(LOG_FILE_NAME)).expect("read log");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Z [INFO] file: Saved scene.epsilon"));
        assert!(lines[1].contains("Z [ERROR] export: Export failed: disk full"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotates_at_the_size_limit_and_keeps_bounded_history() {
        let dir = test_dir("rotate");
        // Each record is far longer than 40 bytes, so every write rotates.
        let writer = LogWriter::new(&dir).with_limits(40, 2);
        for index in 0..6 {
            writer.write(Level::Info, "test", &format!("record number {index}"));
        }

        let current = fs::read_to_string(dir.join(LOG_FILE_NAME)).expect("read current log");
        assert!(current.contains("record number 5"));
        assert!(dir.join("epsilon.log.1").exists());
        assert!(dir.join("epsilon.log.2").exists());
        // Bounded: only the current file plus `keep` siblings.
        assert!(!dir.join("epsilon.log.3").exists());

        fs::remove_dir_all(&dir).ok();
    }
}
