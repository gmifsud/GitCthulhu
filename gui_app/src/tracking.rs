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
}
