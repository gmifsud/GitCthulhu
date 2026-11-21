//! Names the kind of a git error and boils it down to one line for the status strip.
//! Pure: the full text stays in `GitApp::error`.

/// Shown when a view was opened against a HEAD that has since moved.
#[allow(dead_code)] // first used by the HEAD-moved guard (Task 5)
pub(crate) const HEAD_MOVED: &str = "HEAD moved while this was open, so nothing was changed.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Conflict,
    Rejected,
    Auth,
    Network,
    DirtyTree,
    PathTooLong,
    Locked,
    HeadMoved,
    Git,
}

impl ErrorKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ErrorKind::Conflict => "Merge conflict",
            ErrorKind::Rejected => "Push rejected",
            ErrorKind::Auth => "Sign-in failed",
            ErrorKind::Network => "Network",
            ErrorKind::DirtyTree => "Local changes in the way",
            ErrorKind::PathTooLong => "Path too long",
            ErrorKind::Locked => "Repository busy",
            ErrorKind::HeadMoved => "HEAD moved",
            ErrorKind::Git => "Git",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ErrorReport {
    pub kind: ErrorKind,
    pub summary: String,
}

/// Rules in `ErrorKind` declaration order; the first whose needle appears wins.
const RULES: &[(ErrorKind, &[&str])] = &[
    (ErrorKind::Conflict, &["conflict (", "automatic merge failed"]),
    (ErrorKind::Rejected, &["[rejected]", "failed to push some refs"]),
    (
        ErrorKind::Auth,
        &[
            "authentication failed",
            "permission denied (publickey)",
            "could not read username",
            "terminal prompts disabled",
        ],
    ),
    (
        ErrorKind::Network,
        &[
            "could not resolve host",
            "unable to access",
            "connection timed out",
            "connection refused",
            "network is unreachable",
        ],
    ),
    (
        ErrorKind::DirtyTree,
        &[
            "your local changes to the following files would be overwritten",
            "would be overwritten by",
            "please commit your changes or stash them",
        ],
    ),
    (ErrorKind::PathTooLong, &["filename too long", "path too long"]),
    (ErrorKind::Locked, &[".lock': file exists", "another git process seems to be running"]),
    (ErrorKind::HeadMoved, &["head moved while this was open"]),
];

const PROGRESS_PREFIXES: &[&str] = &[
    "Enumerating",
    "Counting",
    "Compressing",
    "Writing objects",
    "remote: ",
];
const MESSAGE_PREFIXES: &[&str] = &["error: ", "fatal: ", "warning: "];

pub(crate) fn present(error: &str) -> ErrorReport {
    ErrorReport {
        kind: kind_of(error),
        summary: summary_of(error),
    }
}

fn kind_of(error: &str) -> ErrorKind {
    let lower = error.to_lowercase();
    RULES
        .iter()
        .find(|(_, needles)| needles.iter().any(|needle| lower.contains(needle)))
        .map(|(kind, _)| *kind)
        .unwrap_or(ErrorKind::Git)
}

fn summary_of(error: &str) -> String {
    let line = error
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with("hint:")
                && !PROGRESS_PREFIXES.iter().any(|p| line.starts_with(p))
        })
        .unwrap_or_else(|| error.trim());
    MESSAGE_PREFIXES
        .iter()
        .find_map(|p| line.strip_prefix(p))
        .unwrap_or(line)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatal_and_hints_are_stripped_from_the_summary() {
        let r = present("hint: see git help\nfatal: refusing to merge unrelated histories\n");
        assert_eq!(r.kind, ErrorKind::Git);
        assert_eq!(r.summary, "refusing to merge unrelated histories");
    }

    #[test]
    fn a_non_fast_forward_push_is_rejected() {
        assert_eq!(
            present("! [rejected]        main -> main (fetch first)\nerror: failed to push some refs").kind,
            ErrorKind::Rejected
        );
        assert_eq!(present("! [rejected] main -> main (stale info)").kind, ErrorKind::Rejected);
    }

    #[test]
    fn auth_network_dirty_lock_and_long_paths_are_named() {
        assert_eq!(present("fatal: Authentication failed for 'https://x'").kind, ErrorKind::Auth);
        assert_eq!(present("git@github.com: Permission denied (publickey).").kind, ErrorKind::Auth);
        assert_eq!(
            present("fatal: unable to access 'https://x/': Could not resolve host: x").kind,
            ErrorKind::Network
        );
        assert_eq!(
            present("error: Your local changes to the following files would be overwritten by checkout:").kind,
            ErrorKind::DirtyTree
        );
        assert_eq!(
            present("fatal: Unable to create '/r/.git/index.lock': File exists.").kind,
            ErrorKind::Locked
        );
        assert_eq!(
            present("error: unable to create file a/b: Filename too long").kind,
            ErrorKind::PathTooLong
        );
        assert_eq!(present("CONFLICT (content): Merge conflict in a.txt").kind, ErrorKind::Conflict);
    }

    #[test]
    fn the_first_non_blank_line_is_the_summary_when_nothing_matches() {
        assert_eq!(present("\n  something odd  \nmore").summary, "something odd");
    }

    #[test]
    fn the_head_moved_copy_is_recognised() {
        assert_eq!(present(HEAD_MOVED).kind, ErrorKind::HeadMoved);
    }

    #[test]
    fn progress_lines_are_not_the_summary() {
        assert_eq!(present("Enumerating objects: 3\nremote: x\nfatal: boom").summary, "boom");
    }
}
