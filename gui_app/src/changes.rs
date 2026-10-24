use std::collections::BTreeMap;

use core_domain::{FileDisposition, RepoStatus};

/// A path the user has marked in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkedFile {
    pub path: String,
    pub place: FileDisposition,
}

/// One line of the working tree, either a folder or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangeRow {
    Directory {
        name: String,
        depth: usize,
    },
    File {
        name: String,
        path: String,
        depth: usize,
        place: FileDisposition,
    },
}

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
    pub place: FileDisposition,
}

/// The non-empty groups of `status`, in the order `git status` prints them.
pub(crate) fn change_sections(status: &RepoStatus) -> Vec<ChangeSection<'_>> {
    [
        (
            "Staged",
            &status.staged_files,
            FileAction::Unstage,
            FileDisposition::Staged,
        ),
        (
            "Unstaged",
            &status.unstaged_files,
            FileAction::Stage,
            FileDisposition::Unstaged,
        ),
        (
            "Untracked",
            &status.untracked_files,
            FileAction::Stage,
            FileDisposition::Untracked,
        ),
    ]
    .into_iter()
    .filter(|(_, files, _, _)| !files.is_empty())
    .map(|(title, files, action, place)| ChangeSection {
        title,
        files: files.as_slice(),
        action,
        place,
    })
    .collect()
}

/// Flat paths, or the same paths folded into folders. Folders are shared.
pub(crate) fn change_rows(
    files: &[String],
    place: FileDisposition,
    as_tree: bool,
) -> Vec<ChangeRow> {
    if !as_tree {
        return files
            .iter()
            .map(|path| ChangeRow::File {
                name: path.clone(),
                path: path.clone(),
                depth: 0,
                place,
            })
            .collect();
    }
    let mut root = Node::default();
    for path in files {
        let parts: Vec<&str> = path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        insert(&mut root, &parts, path);
    }
    let mut rows = Vec::new();
    flatten(&root, 0, place, &mut rows);
    rows
}

/// Paths in `marks` that a Stage or Unstage command should touch.
pub(crate) fn marked_paths(marks: &[MarkedFile], stage: bool) -> Vec<String> {
    marks
        .iter()
        .filter(|mark| {
            if stage {
                mark.place != FileDisposition::Staged
            } else {
                mark.place == FileDisposition::Staged
            }
        })
        .map(|mark| mark.path.clone())
        .collect()
}

#[derive(Default)]
struct Node {
    dirs: BTreeMap<String, Node>,
    files: BTreeMap<String, String>,
}

fn insert(node: &mut Node, parts: &[&str], path: &str) {
    if parts.len() == 1 {
        node.files.insert(parts[0].to_string(), path.to_string());
        return;
    }
    insert(
        node.dirs.entry(parts[0].to_string()).or_default(),
        &parts[1..],
        path,
    );
}

fn flatten(node: &Node, depth: usize, place: FileDisposition, rows: &mut Vec<ChangeRow>) {
    for (name, child) in &node.dirs {
        rows.push(ChangeRow::Directory {
            name: name.clone(),
            depth,
        });
        flatten(child, depth + 1, place, rows);
    }
    for (name, path) in &node.files {
        rows.push(ChangeRow::File {
            name: name.clone(),
            path: path.clone(),
            depth,
            place,
        });
    }
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
    fn a_tree_shares_folders_and_a_list_keeps_the_path() {
        let files = vec![
            "gui_app/src/a.rs".to_string(),
            "gui_app/src/b.rs".to_string(),
        ];
        let listed = change_rows(&files, FileDisposition::Unstaged, false);
        assert!(matches!(
            &listed[0],
            ChangeRow::File { name, .. } if name == "gui_app/src/a.rs"
        ));

        let tree = change_rows(&files, FileDisposition::Unstaged, true);
        assert_eq!(
            tree,
            vec![
                ChangeRow::Directory {
                    name: "gui_app".into(),
                    depth: 0
                },
                ChangeRow::Directory {
                    name: "src".into(),
                    depth: 1
                },
                ChangeRow::File {
                    name: "a.rs".into(),
                    path: "gui_app/src/a.rs".into(),
                    depth: 2,
                    place: FileDisposition::Unstaged,
                },
                ChangeRow::File {
                    name: "b.rs".into(),
                    path: "gui_app/src/b.rs".into(),
                    depth: 2,
                    place: FileDisposition::Unstaged,
                },
            ]
        );
    }

    #[test]
    fn marked_paths_keep_only_the_place_the_command_changes() {
        let marks = vec![
            MarkedFile {
                path: "a.rs".into(),
                place: FileDisposition::Staged,
            },
            MarkedFile {
                path: "b.rs".into(),
                place: FileDisposition::Unstaged,
            },
            MarkedFile {
                path: "c.rs".into(),
                place: FileDisposition::Untracked,
            },
        ];
        assert_eq!(
            marked_paths(&marks, true),
            vec!["b.rs".to_string(), "c.rs".to_string()]
        );
        assert_eq!(marked_paths(&marks, false), vec!["a.rs".to_string()]);
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
                    action: FileAction::Unstage,
                    place: FileDisposition::Staged,
                },
                ChangeSection {
                    title: "Untracked",
                    files: &status.untracked_files,
                    action: FileAction::Stage,
                    place: FileDisposition::Untracked,
                },
            ]
        );
    }
}
