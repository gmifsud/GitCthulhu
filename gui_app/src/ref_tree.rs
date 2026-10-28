//! Branch names folded into folders at each `/`, as the JetBrains branches tree shows them.
//!
//! Pure and deterministic: folders before leaves, each sorted case-insensitively then
//! ordinally, so the same refs always draw the same tree.

/// One row of the flattened tree, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RefRow {
    /// A folder segment. `key` identifies it for collapse state (`<scope>/<path>`).
    Folder {
        key: String,
        label: String,
        depth: usize,
        open: bool,
        leaves: usize,
    },
    /// A ref. `name` is the full ref name; `label` its last segment.
    Leaf {
        name: String,
        label: String,
        depth: usize,
    },
}

#[derive(Default)]
struct Node {
    folders: Vec<(String, Node)>,
    leaves: Vec<(String, String)>,
}

impl Node {
    fn insert(&mut self, segments: &[&str], full: &str) {
        match segments {
            [] => {}
            [leaf] => self.leaves.push(((*leaf).to_string(), full.to_string())),
            [folder, rest @ ..] => {
                let index = match self.folders.iter().position(|(name, _)| name == folder) {
                    Some(index) => index,
                    None => {
                        self.folders.push(((*folder).to_string(), Node::default()));
                        self.folders.len() - 1
                    }
                };
                self.folders[index].1.insert(rest, full);
            }
        }
    }

    fn leaf_count(&self) -> usize {
        self.leaves.len() + self.folders.iter().map(|(_, node)| node.leaf_count()).sum::<usize>()
    }

    fn sort(&mut self) {
        let key = |name: &str| (name.to_lowercase(), name.to_string());
        self.folders.sort_by_key(|(name, _)| key(name));
        self.leaves.sort_by_key(|(label, _)| key(label));
        for (_, node) in &mut self.folders {
            node.sort();
        }
    }
}

/// Flatten `names` under `scope` (e.g. `local`, `remote/origin`) into display rows.
///
/// `is_open(key)` decides each folder's state. While `filter` is non-empty, only refs whose
/// full name contains every whitespace-separated term (case-insensitive) are kept, and every
/// folder on their path is shown open, so a match is never hidden in a closed folder.
pub(crate) fn ref_rows<'n>(
    scope: &str,
    names: impl IntoIterator<Item = &'n str>,
    filter: &str,
    is_open: impl Fn(&str) -> bool,
) -> Vec<RefRow> {
    let terms: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    let mut root = Node::default();
    for name in names {
        let lower = name.to_lowercase();
        if terms.iter().all(|term| lower.contains(term.as_str())) {
            let segments: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
            root.insert(&segments, name);
        }
    }
    root.sort();
    let mut rows = Vec::new();
    flatten(&root, scope, 0, !terms.is_empty(), &is_open, &mut rows);
    rows
}

fn flatten(
    node: &Node,
    prefix: &str,
    depth: usize,
    filtering: bool,
    is_open: &impl Fn(&str) -> bool,
    rows: &mut Vec<RefRow>,
) {
    for (name, child) in &node.folders {
        let key = format!("{prefix}/{name}");
        let open = filtering || is_open(&key);
        rows.push(RefRow::Folder {
            key: key.clone(),
            label: name.clone(),
            depth,
            open,
            leaves: child.leaf_count(),
        });
        if open {
            flatten(child, &key, depth + 1, filtering, is_open, rows);
        }
    }
    for (label, full) in &node.leaves {
        rows.push(RefRow::Leaf {
            name: full.clone(),
            label: label.clone(),
            depth,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[RefRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                RefRow::Folder { label, depth, open, leaves, .. } => {
                    format!("{}{}{label}/ ({leaves})", "  ".repeat(*depth), if *open { "▾" } else { "▸" })
                }
                RefRow::Leaf { label, depth, .. } => format!("{}{label}", "  ".repeat(*depth)),
            })
            .collect()
    }

    #[test]
    fn names_fold_into_folders_before_leaves() {
        let names = ["main", "feature/b", "feature/a", "fix/x/deep", "development"];
        let rows = ref_rows("local", names, "", |_| true);
        assert_eq!(
            labels(&rows),
            [
                "▾feature/ (2)",
                "  a",
                "  b",
                "▾fix/ (1)",
                "  ▾x/ (1)",
                "    deep",
                "development",
                "main",
            ]
        );
    }

    #[test]
    fn a_closed_folder_hides_its_children_but_keeps_its_count() {
        let rows = ref_rows("remote/origin", ["EYC/1", "EYC/2", "main"], "", |key| key != "remote/origin/EYC");
        assert_eq!(labels(&rows), ["▸EYC/ (2)", "main"]);
        assert!(matches!(&rows[0], RefRow::Folder { key, .. } if key == "remote/origin/EYC"));
    }

    #[test]
    fn filtering_keeps_matches_and_opens_their_folders() {
        let rows = ref_rows("local", ["feature/ExternalServices", "feature/other", "main"], "external", |_| false);
        assert_eq!(labels(&rows), ["▾feature/ (1)", "  ExternalServices"]);
    }

    #[test]
    fn leaves_carry_the_full_name() {
        let rows = ref_rows("local", ["feature/a"], "", |_| true);
        assert!(matches!(&rows[1], RefRow::Leaf { name, .. } if name == "feature/a"));
    }
}
