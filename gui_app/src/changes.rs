use std::collections::{BTreeMap, HashSet};

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
        /// Folder path with `/` separators, e.g. `gui_app/src`.
        prefix: String,
        depth: usize,
        /// File descendants not drawn because this folder is collapsed.
        hidden: usize,
        /// Every file under this folder, drawn or not ("3 files").
        files: usize,
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
            "Changes",
            &status.unstaged_files,
            FileAction::Stage,
            FileDisposition::Unstaged,
        ),
        (
            "Unversioned Files",
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
/// `collapsed` holds prefixes (e.g. `gui_app/src`) whose children should be omitted.
pub(crate) fn change_rows(
    files: &[String],
    place: FileDisposition,
    as_tree: bool,
    collapsed: &HashSet<String>,
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
    flatten(&root, 0, "", place, collapsed, &mut rows);
    rows
}

/// Every file row in the order the working tree draws them: sections in `git status` order,
/// folded sections and collapsed folders left out. Shift-click ranges walk this order.
pub(crate) fn visible_files(
    status: &RepoStatus,
    as_tree: bool,
    folded_sections: &HashSet<FileDisposition>,
    collapsed_dirs: &HashSet<(FileDisposition, String)>,
) -> Vec<MarkedFile> {
    let mut order = Vec::new();
    for section in change_sections(status) {
        if folded_sections.contains(&section.place) {
            continue;
        }
        let collapsed: HashSet<String> = collapsed_dirs
            .iter()
            .filter(|(place, _)| *place == section.place)
            .map(|(_, prefix)| prefix.clone())
            .collect();
        for row in change_rows(section.files, section.place, as_tree, &collapsed) {
            if let ChangeRow::File { path, place, .. } = row {
                order.push(MarkedFile { path, place });
            }
        }
    }
    order
}

/// The files from `anchor` to `target` inclusive, in `order`. Either end missing from the
/// order gives just `target`.
pub(crate) fn selection_range(order: &[MarkedFile], anchor: &MarkedFile, target: &MarkedFile) -> Vec<MarkedFile> {
    match (order.iter().position(|f| f == anchor), order.iter().position(|f| f == target)) {
        (Some(a), Some(b)) => order[a.min(b)..=a.max(b)].to_vec(),
        _ => vec![target.clone()],
    }
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

/// Files under `path` in `status` for `place`. A file target returns itself when listed.
pub(crate) fn matching_change_paths(
    status: &RepoStatus,
    place: FileDisposition,
    path: &str,
    is_dir: bool,
) -> Vec<String> {
    let files = match place {
        FileDisposition::Staged => &status.staged_files,
        FileDisposition::Unstaged => &status.unstaged_files,
        FileDisposition::Untracked => &status.untracked_files,
    };
    if !is_dir {
        return files
            .iter()
            .find(|file| file.as_str() == path)
            .cloned()
            .into_iter()
            .collect();
    }
    let prefix = path.trim_end_matches(['/', '\\']);
    let with_slash = format!("{prefix}/");
    files
        .iter()
        .filter(|file| {
            let normalized = file.replace('\\', "/");
            normalized == prefix || normalized.starts_with(&with_slash)
        })
        .cloned()
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

fn file_count(node: &Node) -> usize {
    node.files.len() + node.dirs.values().map(file_count).sum::<usize>()
}

fn join_prefix(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn flatten(
    node: &Node,
    depth: usize,
    prefix: &str,
    place: FileDisposition,
    collapsed: &HashSet<String>,
    rows: &mut Vec<ChangeRow>,
) {
    for (name, child) in &node.dirs {
        // A folder holding only one folder joins it on one row ("src/modules/customer"),
        // as the JetBrains changes tree does, so deep paths do not walk off the right edge.
        let (mut name, mut child) = (name.clone(), child);
        while child.files.is_empty() && child.dirs.len() == 1 {
            let (inner_name, inner) = child.dirs.iter().next().expect("one folder");
            name = format!("{name}/{inner_name}");
            child = inner;
        }
        let child_prefix = join_prefix(prefix, &name);
        let is_collapsed = collapsed.contains(&child_prefix);
        let hidden = if is_collapsed {
            file_count(child)
        } else {
            0
        };
        rows.push(ChangeRow::Directory {
            name,
            prefix: child_prefix.clone(),
            depth,
            hidden,
            files: file_count(child),
        });
        if !is_collapsed {
            flatten(child, depth + 1, &child_prefix, place, collapsed, rows);
        }
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
            head_commit_id: None,
            kinds: Default::default(),
            operation: None,
            conflicted_files: vec![],
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
        assert_eq!(titles, vec!["Staged", "Changes", "Unversioned Files"]);
    }

    #[test]
    fn a_tree_shares_folders_and_a_list_keeps_the_path() {
        let files = vec![
            "gui_app/src/a.rs".to_string(),
            "gui_app/src/b.rs".to_string(),
        ];
        let none = HashSet::new();
        let listed = change_rows(&files, FileDisposition::Unstaged, false, &none);
        assert!(matches!(
            &listed[0],
            ChangeRow::File { name, .. } if name == "gui_app/src/a.rs"
        ));

        let tree = change_rows(&files, FileDisposition::Unstaged, true, &none);
        assert_eq!(
            tree,
            vec![
                ChangeRow::Directory {
                    name: "gui_app/src".into(),
                    prefix: "gui_app/src".into(),
                    depth: 0,
                    hidden: 0,
                    files: 2,
                },
                ChangeRow::File {
                    name: "a.rs".into(),
                    path: "gui_app/src/a.rs".into(),
                    depth: 1,
                    place: FileDisposition::Unstaged,
                },
                ChangeRow::File {
                    name: "b.rs".into(),
                    path: "gui_app/src/b.rs".into(),
                    depth: 1,
                    place: FileDisposition::Unstaged,
                },
            ]
        );
    }

    #[test]
    fn a_collapsed_folder_hides_its_children() {
        let files = vec![
            "gui_app/src/a.rs".to_string(),
            "gui_app/src/b.rs".to_string(),
            "readme.md".to_string(),
        ];
        let collapsed = HashSet::from(["gui_app/src".to_string()]);
        let tree = change_rows(&files, FileDisposition::Unstaged, true, &collapsed);
        assert_eq!(
            tree,
            vec![
                ChangeRow::Directory {
                    name: "gui_app/src".into(),
                    prefix: "gui_app/src".into(),
                    depth: 0,
                    hidden: 2,
                    files: 2,
                },
                ChangeRow::File {
                    name: "readme.md".into(),
                    path: "readme.md".into(),
                    depth: 0,
                    place: FileDisposition::Unstaged,
                },
            ]
        );
    }

    #[test]
    fn shift_ranges_follow_the_drawn_order_across_sections() {
        let status = repo_status(&["s.rs"], &["a.rs", "b.rs"], &["n.rs"]);
        let order = visible_files(&status, false, &HashSet::new(), &HashSet::new());
        let file = |path: &str, place| MarkedFile { path: path.into(), place };
        let picked = selection_range(&order, &file("n.rs", FileDisposition::Untracked), &file("a.rs", FileDisposition::Unstaged));
        assert_eq!(
            picked,
            [file("a.rs", FileDisposition::Unstaged), file("b.rs", FileDisposition::Unstaged), file("n.rs", FileDisposition::Untracked)]
        );
        let folded = HashSet::from([FileDisposition::Unstaged]);
        assert_eq!(visible_files(&status, false, &folded, &HashSet::new()).len(), 2);
    }

    #[test]
    fn a_folder_with_two_children_is_not_joined() {
        let files = vec!["app/src/a.rs".to_string(), "app/tests/b.rs".to_string()];
        let tree = change_rows(&files, FileDisposition::Unstaged, true, &HashSet::new());
        let names: Vec<(usize, &str)> = tree
            .iter()
            .map(|row| match row {
                ChangeRow::Directory { name, depth, .. } | ChangeRow::File { name, depth, .. } => (*depth, name.as_str()),
            })
            .collect();
        assert_eq!(names, [(0, "app"), (1, "src"), (2, "a.rs"), (1, "tests"), (2, "b.rs")]);
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
                    title: "Unversioned Files",
                    files: &status.untracked_files,
                    action: FileAction::Stage,
                    place: FileDisposition::Untracked,
                },
            ]
        );
    }

    #[test]
    fn matching_change_paths_expands_a_folder_prefix() {
        let status = repo_status(
            &[],
            &["scripts/a.sh", "scripts/b.sh", "other.rs"],
            &[],
        );
        assert_eq!(
            matching_change_paths(&status, FileDisposition::Unstaged, "scripts", true),
            vec!["scripts/a.sh".to_string(), "scripts/b.sh".to_string()]
        );
        assert_eq!(
            matching_change_paths(&status, FileDisposition::Unstaged, "other.rs", false),
            vec!["other.rs".to_string()]
        );
    }
}
