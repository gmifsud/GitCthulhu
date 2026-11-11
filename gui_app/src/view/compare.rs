//! The Compare window. For a conflicted file it is the conflict view: yours against theirs,
//! with buttons to take a side. For any other file it compares the working tree, or any
//! revision, with any other revision.

use crate::app::{GitApp, Message};
use crate::compare::{self, CompareSide, CompareState, MergeTool};
use crate::icons::Icon;
use crate::theme::DesignTokens;
use core_domain::ConflictSide;
use iced::overlay::menu;
use iced::widget::{button, column, combo_box, container, horizontal_space, row, scrollable, text, text_input, tooltip, Space};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Theme};

use super::chrome::{accent_button, command_button, field_style, glyph, labelled_button, Reading};

/// Lines drawn before "show more" would be needed. Compare has no paging, so draw plenty.
const LINE_BUDGET: usize = 50_000;
const SIDE_WIDTH_LABEL: f32 = 44.0;

pub(super) fn compare_window<'a>(
    app: &'a GitApp,
    state: &'a CompareState,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let session = app.sessions.iter().find(|session| session.path == state.session_path);
    let status = session.and_then(|session| session.state.current_status.as_ref());
    let conflicted = compare::is_conflicted(status, &state.path);
    let lookups = app.compare_lookups.as_ref();

    let mut title = row![
        glyph(Icon::Compare, reading.size + 2.0, tokens.accent_primary, &reading),
        text(if conflicted { "Resolve conflict" } else { "Compare" })
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..reading.font
            })
            .size(reading.size + 3.0)
            .style(move |_| text::Style { color: Some(tokens.text_primary) }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if conflicted {
        title = title.push(pill("CONFLICT", tokens, reading));
    }
    let head = row![
        title,
        horizontal_space(),
        command_button(
            if app.diff_split { "Unified" } else { "Side by side" },
            Message::ToggleDiffSplit,
            true,
            tokens,
            reading,
        ),
        command_button("Close", Message::CloseCompare, true, tokens, reading),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let file_row = row![
        label("File", tokens, reading),
        text_input("path/to/file", &state.path)
            .font(reading.mono)
            .size(reading.size)
            .padding([4, 8])
            .on_input(Message::ComparePathChanged)
            .on_submit(Message::ComparePathSubmitted)
            .style(field_style(tokens))
            .width(Length::Fill),
        match lookups {
            Some(lookups) => combo_box(&lookups.files, "Find a changed file", None, Message::ComparePathPicked)
                .font(reading.mono)
                .size(reading.size)
                .padding([4, 8])
                .input_style(field_style(tokens))
                .menu_style(lookup_menu_style(tokens))
                .width(Length::Fixed(280.0))
                .into(),
            None => Element::from(Space::with_width(Length::Fixed(280.0))),
        },
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let sides = row![
        side_picker(true, &state.left, &state.left_draft, lookups.map(|lookups| &lookups.left), tokens, reading),
        button(glyph(Icon::SwitchBranch, reading.size + 1.0, tokens.text_secondary, &reading))
            .padding([4, 8])
            .on_press(Message::CompareSwap)
            .style(move |_theme, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(Background::Color(if hot { tokens.border } else { tokens.bg_app })),
                    text_color: tokens.text_primary,
                    border: Border {
                        color: tokens.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    shadow: Shadow::default(),
                }
            }),
        side_picker(false, &state.right, &state.right_draft, lookups.map(|lookups| &lookups.right), tokens, reading),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut card = column![head, file_row, sides].spacing(12);
    if conflicted {
        card = card.push(conflict_actions(&state.path, &state.merge_tool, tokens, reading));
    }
    card = card.push(column_titles(state, tokens, reading));
    card = card.push(body(app, state, tokens, reading));

    container(
        container(card.padding(18).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(tokens.bg_sidebar)),
                text_color: Some(tokens.text_primary),
                border: Border {
                    color: tokens.border,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: Shadow::default(),
            }),
    )
    .padding(28)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(iced::Color { a: 0.75, ..tokens.bg_app })),
        ..container::Style::default()
    })
    .into()
}

fn label<'a>(value: &'a str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(value)
            .font(reading.font)
            .size(reading.size)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
    )
    .width(Length::Fixed(SIDE_WIDTH_LABEL))
    .into()
}

fn pill<'a>(value: &'a str, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    container(
        text(value)
            .font(reading.font)
            .size((reading.size - 3.0).max(9.0))
            .style(|_| text::Style { color: Some(iced::Color::BLACK) }),
    )
    .padding([1, 6])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.removed)),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// One side: a lookup of the working tree, HEAD, stages, branches and tags (type any words of
/// a name, in any order, then pick with the arrows and Enter or the mouse), and a field for any
/// other commit or ref (applied with Enter).
fn side_picker<'a>(
    left: bool,
    current: &CompareSide,
    draft: &str,
    lookup: Option<&'a combo_box::State<CompareSide>>,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let picker: Element<'a, Message> = match lookup {
        Some(lookup) => combo_box(lookup, "Find a branch, tag or HEAD", Some(current), move |side| {
            Message::CompareSidePicked { left, side }
        })
        .font(reading.font)
        .size(reading.size)
        .padding([4, 8])
        .input_style(field_style(tokens))
        .menu_style(lookup_menu_style(tokens))
        .width(Length::Fill)
        .into(),
        None => Space::with_width(Length::Fill).into(),
    };
    let typed = text_input("or type a commit, branch or tag ↵", draft)
        .font(reading.mono)
        .size(reading.size)
        .padding([4, 8])
        .on_input(move |text| Message::CompareDraftChanged { left, text })
        .on_submit(Message::CompareDraftSubmitted { left })
        .style(field_style(tokens))
        .width(Length::Fill);
    column![
        text(if left { "Left" } else { "Right" })
            .font(reading.font)
            .size((reading.size - 1.0).max(10.0))
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        picker,
        typed,
    ]
    .spacing(4)
    .width(Length::FillPortion(1))
    .into()
}

/// The open list under a lookup, in the window's colours rather than iced's grey.
fn lookup_menu_style(tokens: DesignTokens) -> impl Fn(&Theme) -> menu::Style {
    move |_theme| menu::Style {
        background: Background::Color(tokens.bg_app),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: tokens.text_secondary,
        selected_text_color: tokens.text_primary,
        selected_background: Background::Color(tokens.bg_raised),
    }
}

/// Take a side, resolve it in the merge tool, or say the file was fixed by hand. Each closes
/// the window.
fn conflict_actions<'a>(
    path: &str,
    merge_tool: &MergeTool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let take = |side| Message::ResolveConflict { path: path.to_string(), side };
    let open_tool = match merge_tool {
        MergeTool::Named(tool) => {
            labelled_button(Icon::Merge, format!("Open in {tool}"), Message::OpenMergeTool(path.to_string()), true, false, tokens, reading)
        }
        // Disabled, saying why: there is no tool to open, and git's own guess could hang.
        other => {
            let reason = match other {
                MergeTool::Looking => "Looking for a merge tool…".to_string(),
                MergeTool::Failed(error) => format!("Could not read the merge tool: {error}"),
                _ => "No merge tool set up. Set merge.tool in git config, for example: git config --global merge.tool p4merge".to_string(),
            };
            tooltip(
                labelled_button(Icon::Merge, "Open in merge tool".into(), Message::OpenMergeTool(path.to_string()), false, false, tokens, reading),
                container(text(reason).font(reading.font).size((reading.size - 1.0).max(10.0)))
                    .padding([4, 8])
                    .style(move |_theme: &Theme| container::Style {
                        background: Some(Background::Color(tokens.bg_app)),
                        text_color: Some(tokens.text_primary),
                        border: Border { color: tokens.border, width: 1.0, radius: 4.0.into() },
                        ..container::Style::default()
                    }),
                tooltip::Position::Bottom,
            )
            .into()
        }
    };
    container(
        row![
            text("This file has conflicts. Keep one side, resolve it in your merge tool, or edit the file and mark it resolved.")
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style { color: Some(tokens.text_secondary) }),
            horizontal_space(),
            labelled_button(Icon::ArrowSmallUp, "Accept yours".into(), take(ConflictSide::Yours), true, false, tokens, reading),
            labelled_button(Icon::ArrowSmallDown, "Accept theirs".into(), take(ConflictSide::Theirs), true, false, tokens, reading),
            open_tool,
            accent_button("Mark resolved", Message::MarkResolved(path.to_string()), true, tokens, reading),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([6, 10])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.removed_bg)),
        border: Border {
            color: tokens.removed,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// What each half of the diff shows, above it.
fn column_titles<'a>(state: &CompareState, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let title = |side: &CompareSide, removed: bool| {
        container(
            text(side.to_string())
                .font(reading.mono)
                .size(reading.size)
                .style(move |_| text::Style {
                    color: Some(if removed { tokens.removed } else { tokens.added }),
                }),
        )
        .width(Length::FillPortion(1))
        .padding([4, 8])
    };
    row![title(&state.left, true), title(&state.right, false)].into()
}

fn body<'a>(app: &'a GitApp, state: &'a CompareState, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let note = |value: String| -> Element<'a, Message> {
        container(
            text(value)
                .font(reading.font)
                .size(reading.size)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        )
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    if state.path.trim().is_empty() {
        return note("Pick a changed file or type a path, then press Enter.".to_string());
    }
    if compare::query(&state.left, &state.right).is_none() {
        return note("Both sides are the working tree. Pick a revision for one of them.".to_string());
    }
    if state.loading {
        return note("Comparing…".to_string());
    }
    let patch = match &state.result {
        None => return note("Press Enter to compare.".to_string()),
        Some(Err(error)) => return note(error.clone()),
        Some(Ok(patch)) => patch,
    };
    if patch.binary {
        return note("Binary file: the two versions differ.".to_string());
    }
    if patch.hunks.is_empty() {
        return note("No differences: both sides are the same.".to_string());
    }
    let layout = crate::diff_view::layout(std::slice::from_ref(patch), None, LINE_BUDGET, app.diff_split);
    container(scrollable(super::patch::diff_rows(&layout, None, false, tokens, reading)).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_app)),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
