//! Left branch tree, after the JetBrains Git log: one filter box, one scroll, groups for
//! HEAD, Local, Remote, Worktrees, Stashes and Tags, branch names folded into folders at
//! each `/`. A click filters the log to a branch; a double-click checks a local branch out;
//! everything else is on the right-click menu of each row.

use crate::app::{GitApp, Message};
use crate::context_menu::ContextTarget;
use crate::ref_tree::{ref_rows, RefRow};
use crate::selection::Selection;
use crate::session::{basename, same_path, ExplorerSection};
use crate::theme::DesignTokens;
use core_domain::{Branch, Worktree};
use iced::widget::button::Status;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow};

use super::chrome::{
    field_style, glyph, icon_button, one_line, symbol_reading, Reading, BRANCH_MARK, FOLD_CLOSED,
    FOLD_OPEN, REMOTE_MARK, STASH_MARK, TAG_MARK, WORKTREE_MARK,
};

/// Text-input id of the branch-tree filter.
pub(crate) const REF_FILTER_ID: &str = "ref-filter";
const INDENT: f32 = 14.0;
const ROW_PAD_X: f32 = 6.0;
const OPEN_MARK: &str = "●";
const DETACHED: &str = "(detached)";
const FILTER_PLACEHOLDER: &str = "Branch or tag";

pub(super) fn explorer_pane<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let session = app.session();
    let collapsed = session.explorer_collapsed;
    let filter = app.ref_filter.trim();
    let branches = &session.state.branches;
    let head = branches.iter().find(|branch| branch.is_head && !branch.is_remote);
    let locals: Vec<&Branch> = branches.iter().filter(|branch| !branch.is_remote).collect();
    let remotes: Vec<&Branch> = branches.iter().filter(|branch| branch.is_remote).collect();
    let selected = session.selection.as_ref();

    let mut tree = column![].spacing(0).width(Length::Fill);

    // HEAD (current branch): a click shows only what HEAD reaches.
    let head_label = head.map_or_else(|| DETACHED.to_string(), |branch| branch.name.clone());
    tree = tree.push(tree_row(
        0,
        None,
        Some(glyph(BRANCH_MARK, reading.size, tokens.accent_primary, &reading)),
        row![
            label_text("HEAD", tokens.text_primary, reading),
            label_text(&format!("({head_label})"), tokens.text_muted, reading),
        ]
        .spacing(6)
        .into(),
        session.focus_current_branch,
        Message::ToggleFocusCurrentBranch,
        None,
        tokens,
    ));

    // Local
    tree = tree.push(group_header("Local", locals.len(), ExplorerSection::Local, collapsed.local, None, tokens, reading));
    if !collapsed.local {
        let rows = ref_rows("local", locals.iter().map(|branch| branch.name.as_str()), filter, |key| session.ref_folder_open(key));
        for row_data in rows {
            tree = tree.push(branch_tree_row(app, &row_data, false, selected, tokens, reading));
        }
        if locals.is_empty() {
            tree = tree.push(hint_row("No local branches", tokens, reading));
        }
    }

    // Remote: one folder per remote, then the branch's own folders (closed by default).
    tree = tree.push(group_header("Remote", remotes.len(), ExplorerSection::Remote, collapsed.remote, None, tokens, reading));
    if !collapsed.remote {
        let rows = ref_rows("remote", remotes.iter().map(|branch| branch.name.as_str()), filter, |key| session.ref_folder_open(key));
        for row_data in rows {
            tree = tree.push(branch_tree_row(app, &row_data, true, selected, tokens, reading));
        }
        if remotes.is_empty() {
            tree = tree.push(hint_row("No remote branches", tokens, reading));
        }
    }

    // Worktrees
    let worktrees = &app.worktree_catalogue;
    let new_worktree = icon_button(
        "+",
        "New worktree…",
        "worktree-new",
        app.tip_ready("worktree-new"),
        Message::AskCreateWorktree,
        !app.worktree_busy,
        false,
        tokens,
        reading,
    );
    tree = tree.push(group_header("Worktrees", worktrees.len(), ExplorerSection::Worktrees, collapsed.worktrees, Some(new_worktree), tokens, reading));
    if !collapsed.worktrees {
        for entry in worktrees.iter().filter(|entry| matches_filter(filter, &[&entry.path, &entry.branch])) {
            tree = tree.push(worktree_row(app, entry, tokens, reading));
        }
        if worktrees.is_empty() {
            tree = tree.push(hint_row("No worktrees loaded", tokens, reading));
        }
    }

    // Stashes
    let stashes = &session.state.stashes;
    tree = tree.push(group_header("Stashes", stashes.len(), ExplorerSection::Stashes, collapsed.stashes, None, tokens, reading));
    if !collapsed.stashes {
        for stash in stashes.iter().filter(|stash| matches_filter(filter, &[&stash.message])) {
            let is_selected = matches!(selected, Some(Selection::Stash { index, message }) if *index == stash.index && *message == stash.message);
            tree = tree.push(tree_row(
                1,
                None,
                Some(glyph(STASH_MARK, reading.size, tokens.text_muted, &reading)),
                row![
                    label_text(&stash.message, tokens.text_secondary, reading),
                    label_text(&format!("stash@{{{}}}", stash.index), tokens.text_muted, reading),
                ]
                .spacing(6)
                .into(),
                is_selected,
                Message::Select(Selection::Stash {
                    index: stash.index,
                    message: stash.message.clone(),
                }),
                Some(ContextTarget::Stash {
                    index: stash.index,
                    message: stash.message.clone(),
                }),
                tokens,
            ));
        }
        if stashes.is_empty() {
            tree = tree.push(hint_row("No stashes", tokens, reading));
        }
    }

    // Tags
    let tags = &session.state.tags;
    tree = tree.push(group_header("Tags", tags.len(), ExplorerSection::Tags, collapsed.tags, None, tokens, reading));
    if !collapsed.tags {
        for tag in tags.iter().filter(|tag| matches_filter(filter, &[&tag.name])) {
            let is_selected = matches!(selected, Some(Selection::Tag { name, .. }) if *name == tag.name);
            tree = tree.push(tree_row(
                1,
                None,
                Some(glyph(TAG_MARK, reading.size, tokens.added, &reading)),
                label_text(&tag.name, tokens.text_secondary, reading),
                is_selected,
                Message::Select(Selection::Tag {
                    name: tag.name.clone(),
                    commit_id: tag.commit_id.clone(),
                }),
                Some(ContextTarget::Tag {
                    name: tag.name.clone(),
                    commit_id: tag.commit_id.clone(),
                }),
                tokens,
            ));
        }
        if tags.is_empty() {
            tree = tree.push(hint_row("No tags", tokens, reading));
        }
    }

    let filter_box = container(
        text_input(FILTER_PLACEHOLDER, &app.ref_filter)
            .id(REF_FILTER_ID)
            .on_input(Message::RefFilterChanged)
            .font(reading.font)
            .size(reading.size)
            .padding([4, 8])
            .style(field_style(tokens)),
    )
    .padding([4, 6]);

    column![
        filter_box,
        scrollable(tree.padding(Padding { top: 0.0, right: 8.0, bottom: 8.0, left: 0.0 }))
            .direction(Direction::Vertical(Scrollbar::new().width(6).scroller_width(6)))
            .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Every whitespace-separated term appears (case-insensitive) in one of `fields`.
fn matches_filter(filter: &str, fields: &[&str]) -> bool {
    let haystack = fields.join("\n").to_lowercase();
    filter.split_whitespace().all(|term| haystack.contains(&term.to_lowercase()))
}

fn group_header<'a>(
    title: &'static str,
    count: usize,
    section: ExplorerSection,
    collapsed: bool,
    trailing: Option<Element<'a, Message>>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let sym = symbol_reading(&reading);
    let toggle = button(
        row![
            text(if collapsed { FOLD_CLOSED } else { FOLD_OPEN }).font(sym.font).size(reading.size),
            label_text(title, tokens.text_primary, reading),
            label_text(&count.to_string(), tokens.text_muted, reading),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([3.0, ROW_PAD_X])
    .width(Length::Fill)
    .on_press(Message::ToggleExplorerSection(section))
    .style(move |_theme, status| row_style(tokens, status, false));
    let mut header = row![toggle].align_y(Alignment::Center);
    if let Some(trailing) = trailing {
        header = header.push(trailing);
    }
    container(header).padding(Padding { top: 6.0, ..Padding::ZERO }).into()
}

/// A branch-tree row: a folder (toggles) or a branch leaf (click filters the log).
fn branch_tree_row<'a>(
    app: &'a GitApp,
    data: &RefRow,
    remote: bool,
    selected: Option<&Selection>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.session();
    match data {
        RefRow::Folder { key, label, depth, open, leaves } => {
            let sym = symbol_reading(&reading);
            tree_row(
                depth + 1,
                Some(text(if *open { FOLD_OPEN } else { FOLD_CLOSED }).font(sym.font).size(reading.size).into()),
                Some(glyph("▭", reading.size, tokens.text_muted, &reading)),
                row![
                    label_text(label, tokens.text_secondary, reading),
                    label_text(&leaves.to_string(), tokens.text_muted, reading),
                ]
                .spacing(6)
                .into(),
                false,
                Message::ToggleRefFolder(key.clone()),
                None,
                tokens,
            )
        }
        RefRow::Leaf { name, label, depth } => {
            let branch = session
                .state
                .branches
                .iter()
                .find(|branch| branch.is_remote == remote && branch.name == *name);
            let current = branch.is_some_and(|branch| branch.is_head && !branch.is_remote);
            let in_worktree = !remote
                && app
                    .worktree_catalogue
                    .iter()
                    .any(|entry| entry.branch == *name && !same_path(&entry.path, &session.path));
            let is_selected = match selected {
                Some(Selection::Branch { name: chosen, .. }) => !remote && chosen == name,
                Some(Selection::Remote { name: chosen }) => remote && chosen == name,
                _ => false,
            } || session.branch_filter.as_deref() == Some(name.as_str());
            let ink = if current { tokens.accent_primary } else { tokens.text_secondary };
            let mark = if remote { REMOTE_MARK } else if in_worktree { WORKTREE_MARK } else { BRANCH_MARK };
            let mut content = row![label_text(label, ink, reading)].spacing(6).align_y(Alignment::Center);
            if let Some(branch) = branch {
                content = content.push(tracking_badges(branch.ahead, branch.behind, tokens, reading));
            }
            let tip = branch.map(|branch| branch.tip.clone()).unwrap_or_default();
            let target = if remote {
                ContextTarget::RemoteBranch { name: name.clone(), tip }
            } else {
                ContextTarget::LocalBranch { name: name.clone(), tip }
            };
            tree_row(
                depth + 1,
                None,
                Some(glyph(mark, reading.size, ink, &reading)),
                content.into(),
                is_selected,
                Message::RefClicked { name: name.clone(), remote },
                Some(target),
                tokens,
            )
        }
    }
}

/// The one row shape of the tree: indent, optional chevron, mark, one-line content.
#[allow(clippy::too_many_arguments)]
pub(super) fn tree_row<'a>(
    depth: usize,
    chevron: Option<Element<'a, Message>>,
    mark: Option<Element<'a, Message>>,
    content: Element<'a, Message>,
    selected: bool,
    on_press: Message,
    context: Option<ContextTarget>,
    tokens: DesignTokens,
) -> Element<'a, Message> {
    let mut line = row![Space::with_width(Length::Fixed(depth as f32 * INDENT))]
        .spacing(4)
        .align_y(Alignment::Center);
    line = line.push(container(chevron.unwrap_or_else(|| Space::with_width(0).into())).width(Length::Fixed(10.0)));
    if let Some(mark) = mark {
        line = line.push(mark);
    }
    line = line.push(container(content).width(Length::Fill).clip(true));
    let body = button(line)
        .padding([2.0, ROW_PAD_X])
        .width(Length::Fill)
        .on_press(on_press)
        .style(move |_theme, status| row_style(tokens, status, selected));
    match context {
        Some(target) => mouse_area(body).on_right_press(Message::OpenContextMenu { target }).into(),
        None => body.into(),
    }
}

fn row_style(tokens: DesignTokens, status: Status, selected: bool) -> iced::widget::button::Style {
    let hot = matches!(status, Status::Hovered | Status::Pressed);
    iced::widget::button::Style {
        background: Some(Background::Color(if selected {
            tokens.border
        } else if hot {
            tokens.bg_raised
        } else {
            Color::TRANSPARENT
        })),
        text_color: tokens.text_secondary,
        border: Border { radius: 3.0.into(), ..Border::default() },
        shadow: Shadow::default(),
    }
}

pub(super) fn label_text<'a>(value: &str, color: Color, reading: Reading) -> Element<'a, Message> {
    one_line(
        text(value.to_string())
            .font(reading.font)
            .size(reading.size)
            .wrapping(Wrapping::None)
            .style(move |_| iced::widget::text::Style { color: Some(color) }),
        reading.size,
        Length::Shrink,
    )
}

fn hint_row<'a>(label: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(label_text(label, tokens.text_muted, reading))
        .padding(Padding { left: INDENT + 16.0, top: 2.0, bottom: 2.0, right: 0.0 })
        .into()
}

/// One line per worktree: folder name, its branch, main/locked tags. Click opens its tab;
/// right-click offers open, new, folder, copy and remove.
fn worktree_row<'a>(app: &'a GitApp, entry: &'a Worktree, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let open = app.sessions.iter().any(|session| same_path(&session.path, &entry.path));
    let active = app.sessions.get(app.active).is_some_and(|session| same_path(&session.path, &entry.path));
    let branch = if entry.branch.is_empty() { DETACHED.to_string() } else { entry.branch.clone() };
    let (ahead, behind) = tracking_for_worktree(app, entry);
    let mut content = row![
        label_text(basename(&entry.path), if open { tokens.text_primary } else { tokens.text_secondary }, reading),
        label_text(&branch, tokens.accent_primary, reading),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    if entry.is_main {
        content = content.push(wt_tag("main", tokens, reading));
    }
    if entry.locked {
        content = content.push(wt_tag("locked", tokens, reading));
    }
    content = content.push(tracking_badges(ahead, behind, tokens, reading));
    tree_row(
        1,
        None,
        Some(glyph(
            if open { WORKTREE_MARK } else { OPEN_MARK },
            (reading.size - 2.0).max(9.0),
            if open { tokens.accent_primary } else { tokens.text_muted },
            &reading,
        )),
        content.into(),
        active,
        Message::OpenWorktree(entry.path.clone()),
        Some(ContextTarget::Worktree {
            path: entry.path.clone(),
            branch: entry.branch.clone(),
            is_main: entry.is_main,
        }),
        tokens,
    )
}

fn wt_tag<'a>(label: &'static str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    use iced::widget::Theme;
    container(
        text(label)
            .font(reading.font)
            .size((reading.size - 3.0).max(10.0))
            .style(move |_| iced::widget::text::Style {
                color: Some(tokens.text_muted),
            }),
    )
    .padding([0, 4])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Some(tokens.text_muted),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: 3.0.into(),
        },
        shadow: Shadow::default(),
    })
    .into()
}

fn tracking_for_worktree(app: &GitApp, entry: &Worktree) -> (usize, usize) {
    if let Some(session) = app
        .sessions
        .iter()
        .find(|session| same_path(&session.path, &entry.path))
    {
        if let Some(branch) = session
            .state
            .branches
            .iter()
            .find(|b| b.is_head && !b.is_remote)
        {
            return (branch.ahead, branch.behind);
        }
    }
    if entry.branch.is_empty() {
        return (0, 0);
    }
    for session in &app.sessions {
        if let Some(branch) = session
            .state
            .branches
            .iter()
            .find(|b| !b.is_remote && b.name == entry.branch)
        {
            return (branch.ahead, branch.behind);
        }
    }
    (0, 0)
}

fn tracking_badges<'a>(
    ahead: usize,
    behind: usize,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    if ahead == 0 && behind == 0 {
        return Space::with_width(0).into();
    }
    let size = (reading.size - 1.0).max(11.0);
    let mut badges = row![].spacing(4).align_y(Alignment::Center);
    if ahead > 0 {
        badges = badges.push(
            text(format!("+{ahead}"))
                .font(reading.font)
                .size(size)
                .wrapping(Wrapping::None)
                .style(move |_| iced::widget::text::Style {
                    color: Some(tokens.added),
                }),
        );
    }
    if behind > 0 {
        badges = badges.push(
            text(format!("-{behind}"))
                .font(reading.font)
                .size(size)
                .wrapping(Wrapping::None)
                .style(move |_| iced::widget::text::Style {
                    color: Some(tokens.removed),
                }),
        );
    }
    badges.into()
}
