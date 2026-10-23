//! Parse unified diffs and turn one hunk back into a patch `git apply` accepts.

use core_domain::{DiffLine, DiffLineKind, DomainError, Hunk, Patch};

/// Parse `git diff` / `git show` output into one [`Patch`] per file.
pub(crate) fn parse_patches(text: &str) -> Vec<Patch> {
    let mut patches = Vec::new();
    let mut current: Option<Draft> = None;

    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        if let Some(spec) = line.strip_prefix("diff --git ") {
            push_draft(&mut patches, current.take());
            let mut draft = Draft::default();
            if let Some((_, new_path)) = spec.rsplit_once(" b/") {
                draft.path = new_path.trim_matches('"').to_string();
            }
            current = Some(draft);
            continue;
        }
        if line.starts_with("Binary files ") {
            if let Some(draft) = current.as_mut() {
                draft.binary = true;
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("--- ") {
            if let Some(draft) = current.as_mut() {
                draft.old_path = Some(diff_path(path));
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if let Some(draft) = current.as_mut() {
                let new_path = diff_path(path);
                draft.path = if new_path == "/dev/null" {
                    draft.old_path.clone().unwrap_or(new_path)
                } else {
                    new_path
                };
            }
            continue;
        }
        if line.starts_with("@@") {
            if let Some(draft) = current.as_mut() {
                draft.flush_hunk();
                draft.hunk = Some(Hunk {
                    header: line.to_string(),
                    lines: Vec::new(),
                });
            }
            continue;
        }
        if line.starts_with('\\') {
            continue;
        }
        let Some(draft) = current.as_mut() else {
            continue;
        };
        let Some(hunk) = draft.hunk.as_mut() else {
            continue;
        };
        let (kind, text) = match line.chars().next() {
            Some('+') => (DiffLineKind::Added, &line[1..]),
            Some('-') => (DiffLineKind::Removed, &line[1..]),
            Some(' ') => (DiffLineKind::Context, &line[1..]),
            _ => continue,
        };
        hunk.lines.push(DiffLine {
            kind,
            text: text.to_string(),
        });
    }
    push_draft(&mut patches, current);
    patches
}

/// A one-hunk unified diff. A hunk that adds a new file uses `/dev/null` as the old side.
pub(crate) fn render_hunk(path: &str, hunk: &Hunk) -> Result<String, DomainError> {
    if path.is_empty() || path.contains(['\0', '\n', '"']) {
        return Err(DomainError::Unknown(
            "a path is empty or cannot be written into a patch".to_string(),
        ));
    }
    let new_file = hunk.header.split_whitespace().nth(1) == Some("-0,0");
    let mut patch = String::new();
    patch.push_str(&format!("diff --git a/{path} b/{path}\n"));
    if new_file {
        patch.push_str(&format!("--- /dev/null\n+++ b/{path}\n"));
    } else {
        patch.push_str(&format!("--- a/{path}\n+++ b/{path}\n"));
    }
    patch.push_str(&hunk.header);
    patch.push('\n');
    for line in &hunk.lines {
        let mark = match line.kind {
            DiffLineKind::Context => ' ',
            DiffLineKind::Added => '+',
            DiffLineKind::Removed => '-',
        };
        patch.push(mark);
        patch.push_str(&line.text);
        patch.push('\n');
    }
    Ok(patch)
}

/// An untracked worktree file, shown as one added hunk. A NUL byte marks it binary.
pub(crate) fn patch_from_worktree(path: &str, bytes: &[u8]) -> Patch {
    if bytes.contains(&0) {
        return Patch {
            path: path.to_string(),
            hunks: Vec::new(),
            binary: true,
        };
    }
    if bytes.is_empty() {
        return Patch {
            path: path.to_string(),
            hunks: Vec::new(),
            binary: false,
        };
    }
    let text = String::from_utf8_lossy(bytes);
    let mut pieces: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    if text.ends_with('\n') {
        pieces.pop();
    }
    let lines: Vec<DiffLine> = pieces
        .into_iter()
        .map(|line| DiffLine {
            kind: DiffLineKind::Added,
            text: line.to_string(),
        })
        .collect();
    let count = lines.len();
    Patch {
        path: path.to_string(),
        hunks: vec![Hunk {
            header: format!("@@ -0,0 +1,{count} @@"),
            lines,
        }],
        binary: false,
    }
}

#[derive(Default)]
struct Draft {
    path: String,
    old_path: Option<String>,
    hunks: Vec<Hunk>,
    hunk: Option<Hunk>,
    binary: bool,
}

impl Draft {
    fn flush_hunk(&mut self) {
        if let Some(hunk) = self.hunk.take() {
            self.hunks.push(hunk);
        }
    }
}

fn push_draft(patches: &mut Vec<Patch>, draft: Option<Draft>) {
    let Some(mut draft) = draft else {
        return;
    };
    draft.flush_hunk();
    if draft.path.is_empty() && draft.old_path.is_none() && !draft.binary {
        return;
    }
    let path = if draft.path.is_empty() {
        draft.old_path.unwrap_or_default()
    } else {
        draft.path
    };
    if path.is_empty() || path == "/dev/null" {
        return;
    }
    patches.push(Patch {
        path,
        hunks: draft.hunks,
        binary: draft.binary,
    });
}

fn diff_path(raw: &str) -> String {
    let raw = raw.split('\t').next().unwrap_or(raw).trim();
    let raw = if let Some(quoted) = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        quoted
    } else {
        raw
    };
    raw.strip_prefix("b/")
        .or_else(|| raw.strip_prefix("a/"))
        .unwrap_or(raw)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unified_diff_keeps_each_hunk_and_its_path() {
        let text = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,3 @@
 keep
-old
+new
+extra
@@ -8 +9 @@ fn tail
 context
-drop
";
        let patches = parse_patches(text);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].path, "src/a.rs");
        assert!(!patches[0].binary);
        assert_eq!(patches[0].hunks.len(), 2);
        assert_eq!(patches[0].hunks[0].header, "@@ -1,2 +1,3 @@");
        assert_eq!(
            patches[0].hunks[0].lines,
            vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    text: "keep".into()
                },
                DiffLine {
                    kind: DiffLineKind::Removed,
                    text: "old".into()
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    text: "new".into()
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    text: "extra".into()
                },
            ]
        );
        assert_eq!(patches[0].hunks[1].lines.len(), 2);
    }

    #[test]
    fn a_deleted_file_keeps_the_old_path() {
        let text = "\
diff --git a/gone.txt b/gone.txt
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-gone
";
        let patches = parse_patches(text);
        assert_eq!(patches[0].path, "gone.txt");
        assert_eq!(patches[0].hunks[0].lines[0].kind, DiffLineKind::Removed);
    }

    #[test]
    fn a_binary_file_has_no_hunks() {
        let text = "\
diff --git a/pic.bin b/pic.bin
Binary files a/pic.bin and b/pic.bin differ
";
        let patches = parse_patches(text);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].binary);
        assert!(patches[0].hunks.is_empty());
    }

    #[test]
    fn rendering_a_hunk_round_trips_through_the_parser() {
        let hunk = Hunk {
            header: "@@ -1 +1,2 @@".into(),
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Removed,
                    text: "old".into(),
                },
                DiffLine {
                    kind: DiffLineKind::Added,
                    text: "new".into(),
                },
            ],
        };
        let rendered = render_hunk("a.txt", &hunk).expect("render");
        let patches = parse_patches(&rendered);
        assert_eq!(patches[0].path, "a.txt");
        assert_eq!(patches[0].hunks[0], hunk);
    }

    #[test]
    fn an_untracked_file_is_one_added_hunk() {
        let patch = patch_from_worktree("note.txt", b"one\ntwo\n");
        assert_eq!(patch.hunks.len(), 1);
        assert_eq!(patch.hunks[0].lines.len(), 2);
        assert_eq!(patch.hunks[0].lines[0].text, "one");
        assert!(!patch.binary);
        assert!(patch_from_worktree("blob", b"a\0b").binary);
    }
}
