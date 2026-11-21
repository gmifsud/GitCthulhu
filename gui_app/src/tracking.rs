use core_domain::{RepoStatus, TrackingStatus};

/// One line for the current branch. Counts are already computed by git.
///
/// `reading` is whether a read is in flight: with no status and nothing running, the last
/// read failed, and saying "Loading…" for ever would be a lie.
pub(crate) fn tracking_summary(status: Option<&RepoStatus>, reading: bool) -> String {
    match status {
        None if reading => "Loading…".to_string(),
        None => "Not read: press Refresh".to_string(),
        Some(RepoStatus { tracking: None, .. }) => "No upstream".to_string(),
        Some(RepoStatus {
            tracking: Some(tracking),
            ..
        }) => describe(tracking),
    }
}

fn describe(tracking: &TrackingStatus) -> String {
    let upstream = &tracking.upstream;
    // The same ↓ incoming / ↑ outgoing words and order as every badge and button.
    match (tracking.ahead, tracking.behind) {
        (0, 0) => format!("Up to date with {upstream}"),
        (ahead, 0) => format!("↑{ahead} outgoing to {upstream}"),
        (0, behind) => format!("↓{behind} incoming from {upstream}"),
        (ahead, behind) => format!("↓{behind} incoming · ↑{ahead} outgoing · {upstream}"),
    }
}

/// Why a git action cannot start. The top bar's disabled buttons and the command palette
/// say the same words.
pub(crate) const BUSY_REASON: &str = "Another git action is running";
pub(crate) const LOADING_REASON: &str = "Waiting for the repository to load";
pub(crate) const NO_UPSTREAM_REASON: &str =
    "No upstream: this branch does not track a remote branch";
pub(crate) const NO_COMMITS_REASON: &str = "No commits yet";
pub(crate) const NO_REMOTE_REASON: &str = "No remote: add one with git remote add";
pub(crate) const SEVERAL_REMOTES_REASON: &str =
    "Several remotes and none is origin: publish from the terminal";
/// Plain push on a diverged branch: git would refuse it.
pub(crate) const DIVERGED_REASON: &str =
    "The branch and its upstream have diverged: use Force push";
/// A diverged branch the user did not rewrite: the upstream has commits (a colleague's) the
/// branch never held, so a force push would drop them.
pub(crate) const DIVERGED_PULL_REASON: &str = "Diverged: pull or rebase first";
pub(crate) const HAS_UPSTREAM_REASON: &str = "This branch already has an upstream";
pub(crate) const NOT_DIVERGED_REASON: &str =
    "Nothing to overwrite: the branch has not diverged from its upstream";

/// What pressing a sync button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyncAction {
    Fetch,
    Pull,
    Push,
    /// Push the current branch to this remote and track it.
    Publish(String),
    /// Ask first, then force push with a lease.
    ForcePush,
    Stash,
}

/// One button of the header's sync group: its label, whether it can be pressed, why not,
/// and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncButton {
    pub(crate) label: String,
    pub(crate) enabled: bool,
    /// Shown as a tooltip when the button is disabled.
    pub(crate) reason: Option<&'static str>,
    pub(crate) action: SyncAction,
}

/// Which of the three pushes the branch is ready for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushKind {
    Push,
    Publish,
    ForcePush,
}

/// The remote a branch without an upstream is published to: `origin`, otherwise the only one.
pub(crate) fn publish_remote(remotes: &[String]) -> Option<&str> {
    match remotes {
        [only] => Some(only.as_str()),
        _ => remotes.iter().map(String::as_str).find(|name| *name == "origin"),
    }
}

/// The one decision behind the Push button and the palette's Push, Publish and Force push:
/// which push fits the branch, or why none does. Busy and loading are the caller's.
/// A diverged branch is force pushed only when the remote was rewritten under you, that is
/// when you rewrote commits it holds (`rewritten_locally`); otherwise it must pull first.
pub(crate) fn push_kind(status: Option<&RepoStatus>, remotes: &[String]) -> Result<PushKind, &'static str> {
    let status = status.ok_or(LOADING_REASON)?;
    match &status.tracking {
        Some(tracking) if tracking.ahead > 0 && tracking.behind > 0 => {
            if tracking.rewritten_locally {
                Ok(PushKind::ForcePush)
            } else {
                Err(DIVERGED_PULL_REASON)
            }
        }
        Some(_) => Ok(PushKind::Push),
        None if status.head_commit_id.is_none() => Err(NO_COMMITS_REASON),
        None if remotes.is_empty() => Err(NO_REMOTE_REASON),
        None if publish_remote(remotes).is_some() => Ok(PushKind::Publish),
        None => Err(SEVERAL_REMOTES_REASON),
    }
}

/// Fetch, Pull and Push labels from the one tracking state the branch line also reads.
///
/// Counts are arrows plus numbers, never colour alone. A missing status reads as loading
/// ("…"), never as 0; a branch without an upstream says so instead of showing 0. Push turns
/// into "Publish to <remote>" without an upstream and "Force push ↑n" once a branch you
/// rewrote has diverged. `running` is the title of the git action in progress, which every button waits for.
pub(crate) fn sync_buttons(status: Option<&RepoStatus>, remotes: &[String], running: Option<&str>) -> [SyncButton; 3] {
    let busy = running.is_some();
    let progress = |title: &str, doing: &str, idle: String| match running {
        Some(now) if now == title => format!("{doing}…"),
        _ => idle,
    };
    let (pull, upstream_reason) = match status {
        None => ("Pull ↓…".to_string(), Some(LOADING_REASON)),
        Some(RepoStatus { tracking: None, .. }) => ("Pull".to_string(), Some(NO_UPSTREAM_REASON)),
        // A count only when there is something to move: "Pull ↓0" read as noise. The branch
        // line says "Up to date with …" when both are zero.
        Some(RepoStatus { tracking: Some(tracking), .. }) => (
            if tracking.behind > 0 { format!("Pull ↓{}", tracking.behind) } else { "Pull".to_string() },
            None,
        ),
    };
    let ahead = status.and_then(|status| status.tracking.as_ref()).map_or(0, |tracking| tracking.ahead);
    let plan = push_kind(status, remotes);
    let (push_label, push_action) = match (plan, publish_remote(remotes)) {
        (Ok(PushKind::Publish), Some(remote)) => (format!("Publish to {remote}"), SyncAction::Publish(remote.to_string())),
        (Ok(PushKind::ForcePush), _) => (format!("Force push ↑{ahead}"), SyncAction::ForcePush),
        _ if status.is_none() => ("Push ↑…".to_string(), SyncAction::Push),
        _ if ahead > 0 => (format!("Push ↑{ahead}"), SyncAction::Push),
        _ => ("Push".to_string(), SyncAction::Push),
    };
    let push_label = match running {
        Some("Push") => "Pushing…".to_string(),
        Some("Publish") => "Publishing…".to_string(),
        Some("Force push") => "Force pushing…".to_string(),
        _ => push_label,
    };
    let busy_reason = busy.then_some(BUSY_REASON);
    [
        SyncButton {
            label: progress("Fetch", "Fetching", "Fetch".to_string()),
            enabled: !busy && status.is_some(),
            reason: busy_reason.or(status.is_none().then_some(LOADING_REASON)),
            action: SyncAction::Fetch,
        },
        SyncButton {
            label: progress("Pull", "Pulling", pull),
            enabled: !busy && upstream_reason.is_none(),
            reason: busy_reason.or(upstream_reason),
            action: SyncAction::Pull,
        },
        SyncButton {
            label: push_label,
            enabled: !busy && plan.is_ok(),
            reason: busy_reason.or(plan.err()),
            action: push_action,
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
            Some(BUSY_REASON)
        } else if files == 0 {
            Some("No tracked changes to stash")
        } else {
            None
        },
        action: SyncAction::Stash,
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
            rewritten_locally: false,
        }
    }

    /// Diverged because the branch was amended or rebased here after it was pushed.
    fn rewritten(ahead: usize, behind: usize) -> TrackingStatus {
        TrackingStatus { rewritten_locally: true, ..tracked(ahead, behind) }
    }

    #[test]
    fn phrases_match_the_counts() {
        assert_eq!(tracking_summary(None, true), "Loading…");
        assert_eq!(tracking_summary(None, false), "Not read: press Refresh", "a failed read is not loading");
        assert_eq!(tracking_summary(Some(&status(None)), false), "No upstream");
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(0, 0)))), false),
            "Up to date with origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(2, 0)))), false),
            "↑2 outgoing to origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(0, 1)))), false),
            "↓1 incoming from origin/main"
        );
        assert_eq!(
            tracking_summary(Some(&status(Some(tracked(1, 3)))), false),
            "↓3 incoming · ↑1 outgoing · origin/main"
        );
    }

    fn origin() -> Vec<String> {
        vec!["origin".to_string()]
    }

    fn born(tracking: Option<TrackingStatus>) -> RepoStatus {
        RepoStatus {
            head_commit_id: Some("abc".to_string()),
            ..status(tracking)
        }
    }

    fn names(remotes: &[&str]) -> Vec<String> {
        remotes.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn sync_labels_show_counts_with_arrows_and_tell_loading_from_zero() {
        let [_, pull, push] = sync_buttons(Some(&born(Some(tracked(2, 0)))), &origin(), None);
        assert_eq!((pull.label.as_str(), push.label.as_str()), ("Pull", "Push ↑2"));
        assert!(pull.enabled && push.enabled);
        let [_, pull, _] = sync_buttons(Some(&born(Some(tracked(0, 5)))), &origin(), None);
        assert_eq!(pull.label, "Pull ↓5");

        let [_, pull, push] = sync_buttons(Some(&born(Some(tracked(0, 0)))), &origin(), None);
        assert_eq!((pull.label.as_str(), push.label.as_str()), ("Pull", "Push"));

        let [fetch, pull, push] = sync_buttons(None, &origin(), None);
        assert_eq!((pull.label.as_str(), push.label.as_str()), ("Pull ↓…", "Push ↑…"));
        assert!(!fetch.enabled && !pull.enabled && !push.enabled);
    }

    #[test]
    fn no_upstream_disables_pull_and_says_why() {
        let [fetch, pull, _] = sync_buttons(Some(&born(None)), &origin(), None);
        assert!(fetch.enabled);
        assert_eq!(pull.label, "Pull");
        assert!(!pull.enabled);
        assert!(pull.reason.expect("reason").starts_with("No upstream"));
    }

    #[test]
    fn push_reads_publish_without_an_upstream_and_force_push_when_rewritten_and_diverged() {
        struct Case {
            tracking: Option<TrackingStatus>,
            remotes: &'static [&'static str],
            label: &'static str,
            action: SyncAction,
            reason: Option<&'static str>,
        }
        let cases = [
            Case { tracking: None, remotes: &["origin"], label: "Publish to origin", action: SyncAction::Publish("origin".into()), reason: None },
            Case { tracking: None, remotes: &["fork", "origin"], label: "Publish to origin", action: SyncAction::Publish("origin".into()), reason: None },
            Case { tracking: None, remotes: &["fork"], label: "Publish to fork", action: SyncAction::Publish("fork".into()), reason: None },
            Case {
                tracking: None,
                remotes: &["fork", "upstream"],
                label: "Push",
                action: SyncAction::Push,
                reason: Some("Several remotes and none is origin: publish from the terminal"),
            },
            Case { tracking: None, remotes: &[], label: "Push", action: SyncAction::Push, reason: Some("No remote: add one with git remote add") },
            Case { tracking: Some(rewritten(1, 2)), remotes: &["origin"], label: "Force push ↑1", action: SyncAction::ForcePush, reason: None },
            // A colleague pushed, you fetched and committed: the lease would pass and drop theirs.
            Case {
                tracking: Some(tracked(1, 2)),
                remotes: &["origin"],
                label: "Push ↑1",
                action: SyncAction::Push,
                reason: Some("Diverged: pull or rebase first"),
            },
            Case { tracking: Some(tracked(3, 0)), remotes: &["origin"], label: "Push ↑3", action: SyncAction::Push, reason: None },
            Case { tracking: Some(tracked(0, 4)), remotes: &["origin"], label: "Push", action: SyncAction::Push, reason: None },
        ];
        for case in cases {
            let [_, _, push] = sync_buttons(Some(&born(case.tracking.clone())), &names(case.remotes), None);
            assert_eq!(push.label, case.label, "{:?} {:?}", case.tracking, case.remotes);
            assert_eq!(push.action, case.action, "{}", case.label);
            assert_eq!(push.enabled, case.reason.is_none(), "{}", case.label);
            assert_eq!(push.reason, case.reason, "{}", case.label);
        }
    }

    #[test]
    fn publish_is_unavailable_before_the_first_commit() {
        let unborn = status(None);
        let [_, _, push] = sync_buttons(Some(&unborn), &origin(), None);
        assert!(!push.enabled);
        assert_eq!(push.reason, Some("No commits yet"));
        assert_eq!(push_kind(Some(&unborn), &origin()), Err("No commits yet"));
    }

    #[test]
    fn publish_and_force_push_say_what_is_running() {
        let [_, _, push] = sync_buttons(Some(&born(None)), &origin(), Some("Publish"));
        assert_eq!(push.label, "Publishing…");
        let [_, _, push] = sync_buttons(Some(&born(Some(rewritten(1, 1)))), &origin(), Some("Force push"));
        assert_eq!(push.label, "Force pushing…");
        assert!(!push.enabled);
        assert_eq!(push.reason, Some(BUSY_REASON));
    }

    #[test]
    fn a_running_action_names_itself_and_blocks_the_others() {
        let [fetch, pull, push] = sync_buttons(Some(&born(Some(tracked(1, 1)))), &origin(), Some("Pull"));
        assert_eq!(pull.label, "Pulling…");
        assert_eq!(fetch.label, "Fetch");
        assert!(!fetch.enabled && !pull.enabled && !push.enabled);
        assert_eq!(push.reason, Some("Another git action is running"));
    }

    #[test]
    fn each_button_carries_its_action() {
        let [fetch, pull, push] = sync_buttons(Some(&born(Some(tracked(0, 0)))), &origin(), None);
        assert_eq!((fetch.action, pull.action, push.action), (SyncAction::Fetch, SyncAction::Pull, SyncAction::Push));
        assert_eq!(stash_button(Some(&born(None)), None).action, SyncAction::Stash);
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
