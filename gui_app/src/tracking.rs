use core_domain::{RepoStatus, TrackingStatus};

/// One line for the current branch. Counts are already computed by git.
pub(crate) fn tracking_summary(status: Option<&RepoStatus>) -> String {
    match status {
        None => "Loading…".to_string(),
        Some(RepoStatus { tracking: None, .. }) => "No upstream".to_string(),
        Some(RepoStatus {
            tracking: Some(tracking),
            ..
        }) => describe(tracking),
    }
}

fn describe(tracking: &TrackingStatus) -> String {
    let upstream = &tracking.upstream;
    match (tracking.ahead, tracking.behind) {
        (0, 0) => format!("Up to date with {upstream}"),
        (ahead, 0) => format!("{ahead} ahead of {upstream}"),
        (0, behind) => format!("{behind} behind {upstream}"),
        (ahead, behind) => {
            format!("Diverged from {upstream}, {ahead} ahead, {behind} behind")
        }
    }
}

/// One button of the header's sync group: its label, whether it can be pressed, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncButton {
    pub(crate) label: String,
    pub(crate) enabled: bool,
    /// Shown as a tooltip when the button is disabled.
    pub(crate) reason: Option<&'static str>,
}

/// Fetch, Pull and Push labels from the one tracking state the branch line also reads.
///
/// Counts are arrows plus numbers, never colour alone. A missing status reads as loading
/// ("…"), never as 0; a branch without an upstream says so instead of showing 0.
/// `running` is the title of the git action in progress, which every button waits for.
pub(crate) fn sync_buttons(status: Option<&RepoStatus>, running: Option<&str>) -> [SyncButton; 3] {
    let busy = running.is_some();
    let progress = |title: &str, doing: &str, idle: String| match running {
        Some(now) if now == title => format!("{doing}…"),
        _ => idle,
    };
    let (pull, push, upstream_reason) = match status {
        None => ("Pull ↓…".to_string(), "Push ↑…".to_string(), Some("Waiting for the repository to load")),
        Some(RepoStatus { tracking: None, .. }) => (
            "Pull".to_string(),
            "Push".to_string(),
            Some("No upstream: this branch does not track a remote branch"),
        ),
        Some(RepoStatus { tracking: Some(tracking), .. }) => (
            format!("Pull ↓{}", tracking.behind),
            format!("Push ↑{}", tracking.ahead),
            None,
        ),
    };
    let busy_reason = busy.then_some("Another git action is running");
    [
        SyncButton {
            label: progress("Fetch", "Fetching", "Fetch".to_string()),
            enabled: !busy && status.is_some(),
            reason: busy_reason.or(status.is_none().then_some("Waiting for the repository to load")),
        },
        SyncButton {
            label: progress("Pull", "Pulling", pull),
            enabled: !busy && upstream_reason.is_none(),
            reason: busy_reason.or(upstream_reason),
        },
        SyncButton {
            label: progress("Push", "Pushing", push),
            enabled: !busy && upstream_reason.is_none(),
            reason: busy_reason.or(upstream_reason),
        },
    ]
}

/// "Stash (3 files)", or plain "Stash" with a reason when there is nothing to stash.
pub(crate) fn stash_button(status: Option<&RepoStatus>, running: Option<&str>) -> SyncButton {
    let files = status.map_or(0, |status| {
        let mut paths: Vec<&String> = status.staged_files.iter().chain(&status.unstaged_files).collect();
        paths.sort();
        paths.dedup();
        paths.len()
    });
    let busy = running.is_some();
    SyncButton {
        label: match files {
            0 => "Stash".to_string(),
            1 => "Stash (1 file)".to_string(),
            n => format!("Stash ({n} files)"),
        },
        enabled: !busy && files > 0,
        reason: if busy {
            Some("Another git action is running")
        } else if files == 0 {
            Some("No tracked changes to stash")
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(tracking: Option<TrackingStatus>) -> RepoStatus {
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking,
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
        }
    }

    fn tracked(ahead: usize, behind: usize) -> TrackingStatus {
        TrackingStatus {
            upstream: "origin/main".to_string(),
            ahead,
            behind,
        }
    }

    #[test]
    fn phrases_match_the_counts() {
        assert_eq!(tracking_summary(None), "Loading…");
        assert_eq!(tracking_summary(Some(&status(None))), "No upstream");
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(0, 0))))),
            "Up to date with origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(2, 0))))),
            "2 ahead of origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(0, 1))))),
            "1 behind origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(1, 3))))),
            "Diverged from origin/main, 1 ahead, 3 behind"
        );
    }

    #[test]
    fn sync_labels_show_counts_with_arrows_and_tell_loading_from_zero() {
        let [_, pull, push] = sync_buttons(Some(&status(Some(tracked(2, 5)))), None);
        assert_eq!((pull.label.as_str(), push.label.as_str()), ("Pull ↓5", "Push ↑2"));
        assert!(pull.enabled && push.enabled);

        let [_, pull, push] = sync_buttons(Some(&status(Some(tracked(0, 0)))), None);
        assert_eq!((pull.label.as_str(), push.label.as_str()), ("Pull ↓0", "Push ↑0"));

        let [fetch, pull, _] = sync_buttons(None, None);
        assert_eq!(pull.label, "Pull ↓…");
        assert!(!fetch.enabled && !pull.enabled);
    }

    #[test]
    fn no_upstream_disables_pull_and_push_and_says_why() {
        let [fetch, pull, push] = sync_buttons(Some(&status(None)), None);
        assert!(fetch.enabled);
        assert_eq!(pull.label, "Pull");
        assert!(!pull.enabled && !push.enabled);
        assert!(pull.reason.expect("reason").starts_with("No upstream"));
    }

    #[test]
    fn a_running_action_names_itself_and_blocks_the_others() {
        let [fetch, pull, push] = sync_buttons(Some(&status(Some(tracked(1, 1)))), Some("Pull"));
        assert_eq!(pull.label, "Pulling…");
        assert_eq!(fetch.label, "Fetch");
        assert!(!fetch.enabled && !pull.enabled && !push.enabled);
        assert_eq!(push.reason, Some("Another git action is running"));
    }

    #[test]
    fn stash_counts_distinct_changed_files() {
        let mut changed = status(None);
        changed.staged_files = vec!["a".into(), "b".into()];
        changed.unstaged_files = vec!["b".into(), "c".into()];
        assert_eq!(stash_button(Some(&changed), None).label, "Stash (3 files)");
        let clean = stash_button(Some(&status(None)), None);
        assert_eq!(clean.label, "Stash");
        assert!(!clean.enabled);
        assert_eq!(clean.reason, Some("No tracked changes to stash"));
    }
}
