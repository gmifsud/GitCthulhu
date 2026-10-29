//! Right inspector, after the JetBrains Git log.
//!
//! A commit, tag or stash shows its changed files as a folder tree on top and its details
//! below. Anything else (the current branch, a working-tree file) shows the working tree.

use crate::app::{GitApp, Message};
use crate::changes::change_sections;
use crate::dates::full_date;
use crate::ref_pills::refs_at;
use crate::ref_tree::{path_rows, RefRow};
use crate::selection::{short_hash, Selection};
use crate::theme::DesignTokens;
use core_domain::{Commit, RepoStatus};
use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

use super::changes::wip_working_tree_block;
use super::chrome::{glyph, hairline, Reading};
use crate::icons::Icon;
use super::explorer::{label_text, tree_row};
use super::patch::{patch_kind, patch_stats};

/// Collapse-state scope of the changed-files tree, shared with the branch tree's store.
const FILES_SCOPE: &str = "files";
/// Changed files take three parts of the height, details two.
const FILES_PORTION: u16 = 3;
const DETAILS_PORTION: u16 = 2;

pub(crate) fn show_wip_block(status: Option<&RepoStatus>) -> bool {
    status.is_some_and(|status| !change_sections(status).is_empty())
}

/// A selection that names one object in history: its files and details replace the working tree.
fn shows_object(selection: &Selection) -> bool {
    matches!(
        selection,
        Selection::Commit { .. } | Selection::Tag { .. } | Selection::Stash { .. }
    )
}

pub(crate) fn files_navigator<'a>(
    app: &'a GitApp,
    selection: &Selection,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    if shows_object(selection) {
        return column![
            container(scrollable(changed_files_tree(app, tokens, reading)))
                .width(Length::Fill)
                .height(Length::FillPortion(FILES_PORTION)),
            hairline(tokens),
            container(scrollable(details(app, selection, tokens, reading)))
                .width(Length::Fill)
                .height(Length::FillPortion(DETAILS_PORTION)),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }
    let status = app.session().state.current_status.as_ref();
    if !show_wip_block(status) {
        return muted_note("Working tree clean. Select a commit to see its files.", tokens, reading);
    }
    // The block scrolls its own file list; the toolbar and commit box stay put.
    wip_working_tree_block(app, selection, tokens, reading)
}

fn muted_note<'a>(note: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(label_text(note, tokens.text_muted, reading))
        .padding([8, 12])
        .width(Length::Fill)
        .into()
}

/// "N files  +a −r", then the files folded into folders, each with its line counts.
fn changed_files_tree<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    if session.patch_pending {
        return muted_note("Loading files…", tokens, reading);
    }
    if let Some(error) = &session.patch_error {
        return container(label_text(error, tokens.accent_primary, reading))
            .padding([8, 12])
            .into();
    }
    if session.patches.is_empty() {
        return muted_note("No files in this commit", tokens, reading);
    }

    let (total_added, total_removed) = session.patches.iter().fold((0, 0), |(a, r), patch| {
        let (added, removed) = patch_stats(patch);
        (a + added, r + removed)
    });
    let count = session.patches.len();
    let heading = row![
        label_text(
            &format!("{count} {}", if count == 1 { "file" } else { "files" }),
            tokens.text_secondary,
            reading
        ),
        label_text(&format!("+{total_added}"), tokens.added, reading),
        label_text(&format!("−{total_removed}"), tokens.removed, reading),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut tree = column![container(heading).padding([4, 8])].spacing(0).padding([2, 4]);
    let paths = session.patches.iter().map(|patch| patch.path.as_str());
    for data in path_rows(FILES_SCOPE, paths, |key| session.ref_folder_open(key)) {
        tree = tree.push(match data {
            RefRow::Folder { key, label, depth, open, leaves } => tree_row(
                depth,
                Some(glyph(Icon::fold(open), reading.size, tokens.text_muted, &reading)),
                Some(glyph(if open { Icon::FolderOpened } else { Icon::Folder }, reading.size, tokens.text_muted, &reading)),
                row![
                    label_text(&label, tokens.text_secondary, reading),
                    label_text(&leaves.to_string(), tokens.text_muted, reading),
                ]
                .spacing(6)
                .into(),
                false,
                Message::ToggleRefFolder(key),
                None,
                tokens,
            ),
            RefRow::Leaf { name, label, depth } => {
                let patch = session.patches.iter().find(|patch| patch.path == name);
                let (added, removed) = patch.map(patch_stats).unwrap_or((0, 0));
                let binary = patch.is_some_and(|patch| patch.binary);
                let focused = session.focused_diff_path.as_deref() == Some(name.as_str());
                let ink = patch.map_or(tokens.text_primary, |patch| tokens.change_ink(patch_kind(patch)));
                let mut line = row![
                    glyph(Icon::File, reading.size, tokens.text_muted, &reading),
                    label_text(&label, ink, reading),
                    Space::with_width(Length::Fill)
                ]
                    .spacing(6)
                    .align_y(Alignment::Center);
                if binary {
                    line = line.push(label_text("binary", tokens.text_muted, reading));
                } else {
                    if added > 0 {
                        line = line.push(label_text(&format!("+{added}"), tokens.added, reading));
                    }
                    if removed > 0 {
                        line = line.push(label_text(&format!("−{removed}"), tokens.removed, reading));
                    }
                }
                tree_row(depth, None, None, line.into(), focused, Message::FocusDiffPath(name), None, tokens)
            }
        });
    }
    tree.into()
}

/// Subject, hash, author, date, parents and refs, as the JetBrains details pane lists them.
fn details<'a>(app: &'a GitApp, selection: &Selection, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let commit_id = match selection {
        Selection::Commit { id } => Some(id.as_str()),
        Selection::Tag { commit_id, .. } => Some(commit_id.as_str()),
        _ => None,
    };
    let commit = commit_id.and_then(|id| session.state.commits.iter().find(|commit| commit.id == id));
    let mut body = column![].spacing(4).padding([8, 12]).width(Length::Fill);

    match selection {
        Selection::Stash { index, message } => {
            body = body
                .push(subject(message, tokens, reading))
                .push(field("Stash", format!("stash@{{{index}}}"), tokens, reading))
                .push(label_text("Tracked changes against the commit it was taken on.", tokens.text_muted, reading));
            return body.into();
        }
        Selection::Tag { name, .. } => {
            body = body.push(field("Tag", name.clone(), tokens, reading));
        }
        _ => {}
    }
    let Some(commit) = commit else {
        let id = commit_id.unwrap_or_default();
        return body
            .push(field("Commit", id.to_string(), tokens, reading))
            .push(label_text("Not in the loaded history.", tokens.text_muted, reading))
            .into();
    };
    body = body.push(subject(&commit.message, tokens, reading));
    for (label, value) in commit_fields(commit) {
        body = body.push(field(label, value, tokens, reading));
    }
    let refs: Vec<String> = refs_at(&commit.id, &session.state.branches, &session.state.tags, &app.worktree_catalogue)
        .into_iter()
        .map(|pill| pill.label)
        .collect();
    if !refs.is_empty() {
        body = body.push(field("Refs", refs.join(", "), tokens, reading));
    }
    body.into()
}

/// The labelled lines of the details pane, in display order. Pure, for the tests.
fn commit_fields(commit: &Commit) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("Commit", commit.id.clone()),
        ("Author", commit.author.clone()),
        ("Date", full_date(commit.timestamp)),
    ];
    if !commit.parent_ids.is_empty() {
        let label = if commit.parent_ids.len() > 1 { "Parents" } else { "Parent" };
        let parents: Vec<&str> = commit.parent_ids.iter().map(|id| short_hash(id)).collect();
        fields.push((label, parents.join(", ")));
    }
    fields
}

fn subject<'a>(message: &str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    text(message.to_string())
        .font(reading.font)
        .size(reading.size + 1.0)
        .style(move |_| text::Style {
            color: Some(tokens.text_primary),
        })
        .into()
}

/// "Label  value" on one line; long values clip. Copy hash lives on the commit's right-click menu.
fn field<'a>(label: &'static str, value: String, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    row![
        container(label_text(label, tokens.text_muted, reading)).width(Length::Fixed(56.0)),
        container(label_text(&value, tokens.text_secondary, reading)).width(Length::Fill).clip(true),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::RepoStatus;

    fn clean_status() -> RepoStatus {
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec![],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
        }
    }

    fn dirty_status() -> RepoStatus {
        RepoStatus {
            staged_files: vec![],
            unstaged_files: vec!["a.rs".into()],
            untracked_files: vec![],
            tracking: None,
            head_commit_id: None,
            kinds: Default::default(),
        }
    }

    #[test]
    fn wip_block_hidden_when_clean() {
        assert!(!show_wip_block(None));
        assert!(!show_wip_block(Some(&clean_status())));
    }

    #[test]
    fn wip_block_shown_when_dirty() {
        assert!(show_wip_block(Some(&dirty_status())));
    }

    #[test]
    fn only_history_objects_replace_the_working_tree() {
        assert!(shows_object(&Selection::Commit { id: "abc".into() }));
        assert!(shows_object(&Selection::Tag { name: "v1".into(), commit_id: "abc".into() }));
        assert!(shows_object(&Selection::Stash { index: 0, message: "wip".into() }));
        assert!(!shows_object(&Selection::Branch { name: "main".into(), current: false }));
    }

    #[test]
    fn details_list_hash_author_date_and_parents() {
        let commit = Commit {
            id: "0123456789abcdef".into(),
            parent_ids: vec!["aaaaaaaaaa".into(), "bbbbbbbbbb".into()],
            author: "Ada".into(),
            message: "Merge".into(),
            timestamp: 1_793_200_680,
        };
        assert_eq!(
            commit_fields(&commit),
            vec![
                ("Commit", "0123456789abcdef".to_string()),
                ("Author", "Ada".to_string()),
                ("Date", "28 Oct 2026 15:18 UTC".to_string()),
                ("Parents", "aaaaaaa, bbbbbbb".to_string()),
            ]
        );
    }
}
