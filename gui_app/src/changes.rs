use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use core_domain::{ChangeKind, ChangeKinds, FileDisposition, RepoStatus};

/// A path the user has marked in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MarkedFile {
    pub path: String,
    pub place: FileDisposition,
}

/// A kind or place the changes list can be narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChangeChip {
    Staged,
    Unstaged,
    New,
    Modified,
    Deleted,
}

impl ChangeChip {
    pub(crate) const ALL: [ChangeChip; 5] =
        [ChangeChip::Staged, ChangeChip::Unstaged, ChangeChip::New, ChangeChip::Modified, ChangeChip::Deleted];

    pub(crate) fn label(self) -> &'static str {
        match self {
            ChangeChip::Staged => "Staged",
            ChangeChip::Unstaged => "Unstaged",
            ChangeChip::New => "New",
            ChangeChip::Modified => "Modified",
            ChangeChip::Deleted => "Deleted",
        }
    }

    fn matches(self, place: FileDisposition, kind: ChangeKind) -> bool {
        match self {
            ChangeChip::Staged => place == FileDisposition::Staged,
            ChangeChip::Unstaged => place != FileDisposition::Staged,
            ChangeChip::New => matches!(kind, ChangeKind::Added | ChangeKind::Untracked),
            ChangeChip::Modified => matches!(kind, ChangeKind::Modified | ChangeKind::Renamed),
            ChangeChip::Deleted => kind == ChangeKind::Deleted,
        }
    }
}

/// What the changes list is narrowed to. The default shows everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChangeFilter {
    /// A case-insensitive piece of the path.
    pub text: String,
    /// Chips OR together; none chosen matches every file.
    pub chips: BTreeSet<ChangeChip>,
}

impl ChangeFilter {
    pub(crate) fn is_active(&self) -> bool {
        !self.text.is_empty() || !self.chips.is_empty()
    }
}

/// Whether `filter` shows a file. Conflicted files are never hidden.
pub(crate) fn filter_matches(filter: &ChangeFilter, path: &str, place: FileDisposition, kind: ChangeKind) -> bool {
    if kind == ChangeKind::Conflicted {
        return true;
    }
    let text_matches = filter.text.is_empty() || path.to_lowercase().contains(&filter.text.to_lowercase());
    text_matches && (filter.chips.is_empty() || filter.chips.iter().any(|chip| chip.matches(place, kind)))
}

/// The files of one section that `filter` shows. Borrows `files` when nothing is filtered, so
/// the common case copies nothing; callers build this once per section, not per row.
pub(crate) fn filtered_files<'a>(
    files: &'a [String],
    place: FileDisposition,
    kinds: &ChangeKinds,
    filter: &ChangeFilter,
) -> Cow<'a, [String]> {
    if !filter.is_active() {
        return Cow::Borrowed(files);
    }
    Cow::Owned(
        files
            .iter()
            .filter(|path| filter_matches(filter, path, place, kinds.of(place, path)))
            .cloned()
            .collect(),
    )
}

/// Paths a Stage or Unstage should touch: the session's marks, less those the filter hides.
pub(crate) fn filtered_marked_paths(session: &crate::session::WorktreeSession, stage: bool) -> Vec<String> {
    let marks = &session.marked_files;
    let filter = &session.change_filter;
    let Some(status) = session.state.current_status.as_ref().filter(|_| filter.is_active()) else {
        return marked_paths(marks, stage);
    };
    let shown: Vec<MarkedFile> = marks
        .iter()
        .filter(|mark| filter_matches(filter, &mark.path, mark.place, status.kinds.of(mark.place, &mark.path)))
        .cloned()
        .collect();
    marked_paths(&shown, stage)
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
/// folded sections, collapsed folders and files the filter hides left out. Shift-click ranges
/// and the arrow keys walk this order.
pub(crate) fn visible_files(
    status: &RepoStatus,
    as_tree: bool,
    folded_sections: &HashSet<FileDisposition>,
    collapsed_dirs: &HashSet<(FileDisposition, String)>,
    filter: &ChangeFilter,
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
        let files = filtered_files(section.files, section.place, &status.kinds, filter);
        for row in change_rows(&files, section.place, as_tree, &collapsed) {
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
    matching_filtered_paths(status, place, path, is_dir, &ChangeFilter::default())
}

/// [`matching_change_paths`], less the files `filter` hides. Folder actions use this while a
/// filter is active, so they reach only the files the folder row counts.
pub(crate) fn matching_filtered_paths(
    status: &RepoStatus,
    place: FileDisposition,
    path: &str,
    is_dir: bool,
    filter: &ChangeFilter,
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
            (normalized == prefix || normalized.starts_with(&with_slash))
                && filter_matches(filter, file, place, status.kinds.of(place, file))
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
        let order = visible_files(&status, false, &HashSet::new(), &HashSet::new(), &ChangeFilter::default());
        let file = |path: &str, place| MarkedFile { path: path.into(), place };
        let picked = selection_range(&order, &file("n.rs", FileDisposition::Untracked), &file("a.rs", FileDisposition::Unstaged));
        assert_eq!(
            picked,
            [file("a.rs", FileDisposition::Unstaged), file("b.rs", FileDisposition::Unstaged), file("n.rs", FileDisposition::Untracked)]
        );
        let folded = HashSet::from([FileDisposition::Unstaged]);
        assert_eq!(visible_files(&status, false, &folded, &HashSet::new(), &ChangeFilter::default()).len(), 2);
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

    fn filter(text: &str, chips: &[ChangeChip]) -> ChangeFilter {
        ChangeFilter { text: text.into(), chips: chips.iter().copied().collect() }
    }

    #[test]
    fn text_and_chips_filter_and_conflicts_always_show() {
        use ChangeChip::*;
        use FileDisposition::{Staged as InIndex, Unstaged as InTree, Untracked as Loose};
        let table = [
            (filter("", &[]), "a.rs", InTree, ChangeKind::Modified, true),
            (filter("RS", &[]), "src/A.rs", InTree, ChangeKind::Modified, true),
            (filter("rs", &[]), "b.md", InTree, ChangeKind::Modified, false),
            (filter("", &[Staged]), "a.rs", InIndex, ChangeKind::Modified, true),
            (filter("", &[Staged]), "a.rs", InTree, ChangeKind::Modified, false),
            (filter("", &[Unstaged]), "a.rs", InTree, ChangeKind::Modified, true),
            (filter("", &[Unstaged]), "a.rs", Loose, ChangeKind::Untracked, true),
            (filter("", &[Unstaged]), "a.rs", InIndex, ChangeKind::Modified, false),
            (filter("", &[New]), "a.rs", Loose, ChangeKind::Untracked, true),
            (filter("", &[New]), "a.rs", InIndex, ChangeKind::Added, true),
            (filter("", &[New]), "a.rs", InTree, ChangeKind::Modified, false),
            (filter("", &[Modified]), "a.rs", InTree, ChangeKind::Renamed, true),
            (filter("", &[Modified]), "a.rs", InTree, ChangeKind::Deleted, false),
            (filter("", &[Deleted]), "a.rs", InTree, ChangeKind::Deleted, true),
            // Chips OR together; text and chips AND.
            (filter("", &[Deleted, New]), "a.rs", Loose, ChangeKind::Untracked, true),
            (filter("md", &[Deleted, New]), "a.rs", Loose, ChangeKind::Untracked, false),
            (filter("zzz", &[Deleted]), "a.rs", InTree, ChangeKind::Conflicted, true),
        ];
        for (filter, path, place, kind, shown) in table {
            assert_eq!(filter_matches(&filter, path, place, kind), shown, "{filter:?} on {path} {place:?} {kind:?}");
        }
    }

    #[test]
    fn shift_ranges_walk_only_the_files_the_filter_shows() {
        let status = repo_status(&["s.rs"], &["a.rs", "b.md", "c.rs"], &[]);
        let order = visible_files(&status, false, &HashSet::new(), &HashSet::new(), &filter("rs", &[]));
        let paths: Vec<&str> = order.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["s.rs", "a.rs", "c.rs"]);
    }

    #[test]
    fn a_filter_keeps_the_folder_chain_of_the_files_it_shows() {
        let files = vec!["app/src/a.rs".to_string(), "app/tests/b.md".to_string()];
        let kept = filtered_files(&files, FileDisposition::Unstaged, &ChangeKinds::default(), &filter("rs", &[]));
        assert_eq!(kept.as_ref(), ["app/src/a.rs".to_string()]);
        let rows = change_rows(&kept, FileDisposition::Unstaged, true, &HashSet::new());
        assert!(matches!(&rows[0], ChangeRow::Directory { name, files: 1, .. } if name == "app/src"));
    }
}
