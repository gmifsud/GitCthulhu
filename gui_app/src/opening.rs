//! Opening a repository: progress while it opens, and an actionable failure when it does not.
//!
//! The open runs off-thread in stages. The current workspace is replaced only after every stage
//! has succeeded, so a folder that cannot be opened never costs the user their open tabs.

use std::time::{Duration, Instant};

use core_domain::{DomainError, ViewEvent};

/// The running build, for diagnostics.
pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How far an open got. A failure names the stage it stopped in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenStage {
    /// Finding the repository and building its adapter.
    Opening,
    /// Reading status, history, branches, stashes and tags.
    Reading,
}

impl OpenStage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Opening => "opening the repository",
            Self::Reading => "reading the repository",
        }
    }
}

/// An open in progress, for the loading state.
#[derive(Debug, Clone)]
pub(crate) struct OpenProgress {
    pub(crate) path: String,
    pub(crate) started: Instant,
}

impl OpenProgress {
    pub(crate) fn new(path: String) -> Self {
        Self {
            path,
            started: Instant::now(),
        }
    }
}

/// Why an open stopped, with what is needed to retry it and to report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenFailure {
    pub(crate) path: String,
    pub(crate) stage: OpenStage,
    /// Git's or the adapter's message, already redacted.
    pub(crate) message: String,
    pub(crate) duration_ms: u64,
    /// The last operation that finished in this window before the open, if any.
    pub(crate) last_completed: Option<String>,
}

impl OpenFailure {
    pub(crate) fn new(path: &str, stage: OpenStage, error: &DomainError, elapsed: Duration) -> Self {
        Self {
            path: path.to_string(),
            stage,
            message: redact(&crate::jobs::error_message(error)),
            duration_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            last_completed: None,
        }
    }

    /// One line for the banner.
    pub(crate) fn summary(&self) -> String {
        let first = self
            .message
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("unknown error");
        format!("Could not open {} while {}: {first}", self.path, self.stage.label())
    }

    /// Plain text to paste into a bug report. Paths and git's message only; URL credentials
    /// are removed and no setting or token is included.
    pub(crate) fn diagnostics(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!("GitCthulhu {APP_VERSION} ({})\n", std::env::consts::OS));
        text.push_str("Operation: open repository\n");
        text.push_str(&format!("Stage: {}\n", self.stage.label()));
        text.push_str(&format!("Repository: {}\n", redact(&self.path)));
        text.push_str(&format!("Duration: {} ms\n", self.duration_ms));
        text.push_str(&format!(
            "Last completed operation: {}\n",
            self.last_completed.as_deref().unwrap_or("none")
        ));
        text.push_str("Error:\n");
        for line in self.message.lines() {
            text.push_str("  ");
            text.push_str(line);
            text.push('\n');
        }
        text
    }
}

/// What a successful open hands back: the adapter and its first load.
#[derive(Clone)]
pub(crate) struct OpenedWorkspace {
    pub(crate) repo: crate::jobs::SharedRepository,
    pub(crate) events: Vec<ViewEvent>,
}

/// The adapter is a trait object; this keeps `Message` `Debug`.
impl std::fmt::Debug for OpenedWorkspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedWorkspace")
    }
}

/// Open `path` and read it, stopping at the first stage that fails. Blocking; run off-thread.
///
/// Reading fails the open only when status cannot be read: without status there is nothing to
/// show. Errors from history, branches, stashes or tags arrive with the load and are shown in
/// the opened workspace like any other load error.
pub(crate) fn open_and_read(
    open: &crate::RepositoryOpener,
    path: &str,
) -> Result<OpenedWorkspace, OpenFailure> {
    let started = Instant::now();
    let repo = open(path).map_err(|error| OpenFailure::new(path, OpenStage::Opening, &error, started.elapsed()))?;
    let events = crate::jobs::load_repository(repo.as_ref());
    if let Some(ViewEvent::ErrorOccurred(error)) = events.first() {
        return Err(OpenFailure::new(path, OpenStage::Reading, error, started.elapsed()));
    }
    Ok(OpenedWorkspace { repo, events })
}

/// Remove `user:password@` from anything that looks like a URL, so a remote's credentials never
/// reach the screen or the clipboard.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (before, after) = rest.split_at(at + 3);
        out.push_str(before);
        let end = after
            .find(|c: char| c.is_whitespace() || c == '/' || c == '\'' || c == '"')
            .unwrap_or(after.len());
        let authority = &after[..end];
        match authority.rfind('@') {
            Some(sign) => {
                out.push_str("***@");
                out.push_str(&authority[sign + 1..]);
            }
            None => out.push_str(authority),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use test_support::FakeGitRepository;

    fn failure(message: &str) -> OpenFailure {
        OpenFailure {
            path: r"D:\répo".to_string(),
            stage: OpenStage::Reading,
            message: message.to_string(),
            duration_ms: 42,
            last_completed: Some("Fetch".to_string()),
        }
    }

    #[test]
    fn redact_removes_url_credentials_and_keeps_the_rest() {
        assert_eq!(
            redact("fatal: unable to access 'https://bob:ghp_secret@github.com/a/b.git/': 403"),
            "fatal: unable to access 'https://***@github.com/a/b.git/': 403"
        );
        assert_eq!(redact("ssh://git@host:22/repo"), "ssh://***@host:22/repo");
        assert_eq!(redact("no url here, D:\\répo"), "no url here, D:\\répo");
        assert_eq!(redact("https://example.com/x"), "https://example.com/x");
    }

    #[test]
    fn diagnostics_name_version_stage_path_duration_and_last_operation() {
        let text = failure("fatal: bad object HEAD\nsecond line").diagnostics();
        assert!(text.starts_with(&format!("GitCthulhu {APP_VERSION}")));
        assert!(text.contains("Stage: reading the repository"));
        assert!(text.contains(r"Repository: D:\répo"));
        assert!(text.contains("Duration: 42 ms"));
        assert!(text.contains("Last completed operation: Fetch"));
        assert!(text.contains("  fatal: bad object HEAD\n  second line\n"));
    }

    #[test]
    fn summary_is_the_first_non_blank_line() {
        assert_eq!(
            failure("\n  fatal: bad object HEAD\nhint").summary(),
            r"Could not open D:\répo while reading the repository: fatal: bad object HEAD"
        );
    }

    #[test]
    fn a_failing_opener_stops_at_the_opening_stage() {
        let open: crate::RepositoryOpener = Arc::new(|_| Err(DomainError::RepositoryNotFound));
        let failure = open_and_read(&open, r"D:\nowhere").expect_err("must fail");
        assert_eq!(failure.stage, OpenStage::Opening);
        assert_eq!(failure.path, r"D:\nowhere");
    }

    #[test]
    fn unreadable_status_stops_at_the_reading_stage() {
        let open: crate::RepositoryOpener = Arc::new(|_| {
            let repo = FakeGitRepository {
                status_result: Err(DomainError::Unknown("fatal: index file corrupt".to_string())),
                ..FakeGitRepository::ok()
            };
            Ok(Arc::new(repo))
        });
        let failure = open_and_read(&open, r"D:\broken").expect_err("must fail");
        assert_eq!(failure.stage, OpenStage::Reading);
        assert!(failure.message.contains("index file corrupt"));
    }

    #[test]
    fn a_readable_repository_comes_back_with_its_first_load() {
        let open: crate::RepositoryOpener = Arc::new(|_| Ok(Arc::new(FakeGitRepository::ok())));
        let opened = open_and_read(&open, r"D:\repo").expect("opens");
        assert!(matches!(opened.events.first(), Some(ViewEvent::StatusUpdated(_))));
    }
}
