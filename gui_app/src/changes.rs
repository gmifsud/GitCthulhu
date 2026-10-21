use core_domain::RepoStatus;

/// Whether a changes-panel row stages or unstages its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileAction {
    Stage,
    Unstage,
}

/// One group of changed files in the changes panel.
#[derive(Debug, PartialEq)]
pub(crate) struct ChangeSection<'a> {
    pub title: &'static str,
    pub files: &'a [String],
    pub action: FileAction,
}

/// The non-empty groups of `status`, in the order `git status` prints them.
pub(crate) fn change_sections(status: &RepoStatus) -> Vec<ChangeSection<'_>> {
    [
        ("Staged", &status.staged_files, FileAction::Unstage),
        ("Unstaged", &status.unstaged_files, FileAction::Stage),
        ("Untracked", &status.untracked_files, FileAction::Stage),
    ]
    .into_iter()
    .filter(|(_, files, _)| !files.is_empty())
    .map(|(title, files, action)| ChangeSection {
        title,
        files: files.as_slice(),
        action,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_status(staged: &[&str], unstaged: &[&str], untracked: &[&str]) -> RepoStatus {
        let owned = |files: &[&str]| files.iter().map(|file| file.to_string()).collect();
        RepoStatus {
            staged_files: owned(staged),
            unstaged_files: owned(unstaged),
            untracked_files: owned(untracked),
            tracking: None,
        }
    }

    #[test]
    fn clean_status_has_no_sections() {
        assert!(change_sections(&repo_status(&[], &[], &[])).is_empty());
    }

    #[test]
    fn sections_follow_git_status_order() {
        let status = repo_status(&["a.rs"], &["b.rs"], &["c.rs"]);
        let titles: Vec<_> = change_sections(&status)
            .iter()
            .map(|section| section.title)
            .collect();
        assert_eq!(titles, vec!["Staged", "Unstaged", "Untracked"]);
    }

    #[test]
    fn empty_groups_are_left_out() {
        let status = repo_status(&["a.rs"], &[], &["c.rs", "d.rs"]);
        let sections = change_sections(&status);
        assert_eq!(
            sections,
            vec![
                ChangeSection {
                    title: "Staged",
                    files: &status.staged_files,
                    action: FileAction::Unstage
                },
                ChangeSection {
                    title: "Untracked",
                    files: &status.untracked_files,
                    action: FileAction::Stage
                },
            ]
        );
    }
}
