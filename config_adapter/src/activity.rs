//! Each repository's activity log as a JSON-lines file in the app's config folder.
//!
//! One file per repository, named by a hash of its folder, so logs never mix and a repository
//! moved to another folder starts a fresh log. One record per line: an append is one write, and
//! a damaged line is skipped instead of losing the whole log.

use std::io::Write;
use std::path::{Path, PathBuf};

use core_domain::{repository_key, ActivityRecord, ActivityStore, DomainError};

/// Lines kept when a file is compacted.
const KEEP: usize = 500;
/// A file past this many bytes is compacted to [`KEEP`] lines on the next append.
const COMPACT_AT: u64 = 512 * 1024;

pub struct JsonlActivityStore {
    folder: PathBuf,
}

impl JsonlActivityStore {
    /// Logs live in `activity/` beside the settings file of `app_name`.
    pub fn new(app_name: &str) -> Result<Self, DomainError> {
        let settings = confy::get_configuration_file_path(app_name, None)
            .map_err(|error| DomainError::Unknown(format!("Failed to get config path: {error}")))?;
        let parent = settings.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(Self::in_folder(parent.join("activity")))
    }

    pub fn in_folder(folder: PathBuf) -> Self {
        Self { folder }
    }

    fn file_for(&self, repository: &str) -> PathBuf {
        self.folder.join(format!("{:016x}.jsonl", fnv1a(repository_key(repository).as_bytes())))
    }
}

impl ActivityStore for JsonlActivityStore {
    fn load_activity(&self, repository: &str, limit: usize) -> Result<Vec<ActivityRecord>, DomainError> {
        let file = self.file_for(repository);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(unknown("read", &file, error)),
        };
        let records: Vec<ActivityRecord> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let skip = records.len().saturating_sub(limit);
        Ok(records.into_iter().skip(skip).collect())
    }

    fn append_activity(&self, repository: &str, record: &ActivityRecord) -> Result<(), DomainError> {
        std::fs::create_dir_all(&self.folder).map_err(|error| unknown("create", &self.folder, error))?;
        let file = self.file_for(repository);
        let line = serde_json::to_string(record)
            .map_err(|error| DomainError::Unknown(format!("Failed to encode activity: {error}")))?;
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|error| unknown("open", &file, error))?;
        writeln!(handle, "{line}").map_err(|error| unknown("write", &file, error))?;
        drop(handle);
        if std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or(0) > COMPACT_AT {
            self.compact(repository, &file)?;
        }
        Ok(())
    }

    fn clear_activity(&self, repository: &str) -> Result<(), DomainError> {
        let file = self.file_for(repository);
        match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(unknown("remove", &file, error)),
        }
    }
}

impl JsonlActivityStore {
    /// Rewrite the file with its newest [`KEEP`] records, through a temporary file so a crash
    /// halfway leaves the old log whole.
    fn compact(&self, repository: &str, file: &Path) -> Result<(), DomainError> {
        let kept = self.load_activity(repository, KEEP)?;
        let mut body = String::new();
        for record in &kept {
            if let Ok(line) = serde_json::to_string(record) {
                body.push_str(&line);
                body.push('\n');
            }
        }
        let temporary = file.with_extension("jsonl.tmp");
        std::fs::write(&temporary, body).map_err(|error| unknown("write", &temporary, error))?;
        std::fs::rename(&temporary, file).map_err(|error| unknown("replace", file, error))
    }
}

/// 64-bit FNV-1a: stable across runs and platforms, unlike the standard library's hasher.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn unknown(verb: &str, path: &Path, error: std::io::Error) -> DomainError {
    DomainError::Unknown(format!("Failed to {verb} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::ActivityOutcome;

    fn record(title: &str, outcome: ActivityOutcome) -> ActivityRecord {
        ActivityRecord {
            started: 1_793_200_680,
            duration_ms: 1_250,
            title: title.to_string(),
            detail: "development".to_string(),
            command: "git push".to_string(),
            worktree: String::new(),
            outcome,
        }
    }

    fn store(label: &str) -> (JsonlActivityStore, PathBuf) {
        let folder = std::env::temp_dir().join(format!("gitcthulhu-activity-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        (JsonlActivityStore::in_folder(folder.clone()), folder)
    }

    #[test]
    fn records_round_trip_per_repository_and_spelling_does_not_matter() {
        let (store, folder) = store("round");
        store.append_activity(r"D:\Repos\A", &record("Push", ActivityOutcome::Succeeded)).unwrap();
        store
            .append_activity("D:/Repos/A/", &record("Pull", ActivityOutcome::Failed("fatal: no".into())))
            .unwrap();
        store.append_activity(r"D:\Repos\B", &record("Fetch", ActivityOutcome::Stopped)).unwrap();

        let a = store.load_activity(r"D:\Repos\A", 10).unwrap();
        assert_eq!(a.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), vec!["Push", "Pull"]);
        assert_eq!(a[1].outcome, ActivityOutcome::Failed("fatal: no".into()));
        assert_eq!(store.load_activity(r"D:\Repos\B", 10).unwrap().len(), 1);
        assert_eq!(store.load_activity(r"D:\Repos\A", 1).unwrap()[0].title, "Pull", "the newest are kept");

        store.clear_activity(r"D:\Repos\A").unwrap();
        assert!(store.load_activity(r"D:\Repos\A", 10).unwrap().is_empty());
        assert!(store.load_activity(r"D:\Repos\None", 10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_damaged_line_is_skipped_not_fatal() {
        let (store, folder) = store("damaged");
        store.append_activity("/r", &record("Push", ActivityOutcome::Succeeded)).unwrap();
        let file = store.file_for("/r");
        let mut handle = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(handle, "{{not json").unwrap();
        drop(handle);
        store.append_activity("/r", &record("Pull", ActivityOutcome::Succeeded)).unwrap();
        assert_eq!(store.load_activity("/r", 10).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_hash_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
