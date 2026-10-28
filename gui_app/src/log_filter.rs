//! Choices of the log's filter bar (Branch ▾, User ▾), after the JetBrains Git log.
//!
//! Pure: the options come from the loaded branches and commits, in a fixed order, so the
//! same repository always offers the same lists.

use core_domain::{Branch, Commit};
use std::collections::BTreeSet;
use std::fmt;

/// One entry of a filter picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LogChoice {
    /// No filter.
    All,
    /// What the current HEAD reaches (the focus toggle).
    Head,
    /// A branch name, or an author.
    Named(String),
}

const ALL_LABEL: &str = "All";
const HEAD_LABEL: &str = "HEAD (current)";

impl fmt::Display for LogChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogChoice::All => f.write_str(ALL_LABEL),
            LogChoice::Head => f.write_str(HEAD_LABEL),
            LogChoice::Named(name) => f.write_str(name),
        }
    }
}

/// All, HEAD, the local branches, then the remote ones; each group sorted by name.
pub(crate) fn branch_choices(branches: &[Branch]) -> Vec<LogChoice> {
    let names = |remote: bool| -> BTreeSet<&str> {
        branches
            .iter()
            .filter(|branch| branch.is_remote == remote)
            .map(|branch| branch.name.as_str())
            .collect()
    };
    [LogChoice::All, LogChoice::Head]
        .into_iter()
        .chain(names(false).into_iter().map(|name| LogChoice::Named(name.to_string())))
        .chain(names(true).into_iter().map(|name| LogChoice::Named(name.to_string())))
        .collect()
}

/// All, then every author of the loaded commits, sorted and without repeats.
pub(crate) fn author_choices(commits: &[Commit]) -> Vec<LogChoice> {
    let authors: BTreeSet<&str> = commits.iter().map(|commit| commit.author.as_str()).collect();
    std::iter::once(LogChoice::All)
        .chain(authors.into_iter().map(|author| LogChoice::Named(author.to_string())))
        .collect()
}

/// The picker's current value for the branch filter.
pub(crate) fn branch_choice(focus: bool, filter: Option<&str>) -> LogChoice {
    match (focus, filter) {
        (true, _) => LogChoice::Head,
        (false, Some(name)) => LogChoice::Named(name.to_string()),
        (false, None) => LogChoice::All,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, remote: bool) -> Branch {
        Branch {
            name: name.into(),
            is_head: false,
            is_remote: remote,
            tip: "abc".into(),
            ahead: 0,
            behind: 0,
        }
    }

    fn commit(author: &str) -> Commit {
        Commit {
            id: author.into(),
            parent_ids: vec![],
            author: author.into(),
            message: String::new(),
            timestamp: 0,
        }
    }

    #[test]
    fn branches_list_all_head_locals_then_remotes() {
        let choices = branch_choices(&[branch("origin/main", true), branch("main", false), branch("dev", false)]);
        let labels: Vec<String> = choices.iter().map(ToString::to_string).collect();
        assert_eq!(labels, ["All", "HEAD (current)", "dev", "main", "origin/main"]);
    }

    #[test]
    fn authors_are_sorted_once_each() {
        let choices = author_choices(&[commit("Bo"), commit("Ada"), commit("Bo")]);
        assert_eq!(
            choices,
            [LogChoice::All, LogChoice::Named("Ada".into()), LogChoice::Named("Bo".into())]
        );
    }

    #[test]
    fn focus_reads_as_head_in_the_picker() {
        assert_eq!(branch_choice(true, Some("dev")), LogChoice::Head);
        assert_eq!(branch_choice(false, Some("dev")), LogChoice::Named("dev".into()));
        assert_eq!(branch_choice(false, None), LogChoice::All);
    }
}
