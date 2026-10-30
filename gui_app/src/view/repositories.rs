//! The Repositories column: every repository opened in the app, one click from being opened
//! again. Pinned, it is a column of its own left of the Explorer. Unpinned, it folds to a rail
//! of initials, and the rail's top button shows the full list over the window.

use crate::app::{GitApp, Message, REPOSITORY_FILTER_ID};
use crate::icons::Icon;
use crate::session::{basename, same_path};
use crate::theme::DesignTokens;
use iced::widget::{button, column, container, horizontal_space, mouse_area, row, scrollable, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme};

use super::chrome::{embedded_field_style, glyph, icon_button, one_line, Reading};

/// Width of the pinned column and of the flyout.
pub(super) const COLUMN_WIDTH: f32 = 236.0;
/// Width of the folded rail.
pub(super) const RAIL_WIDTH: f32 = 40.0;
const TILE_SIZE: f32 = 24.0;
const ROW_HEIGHT: f32 = 40.0;

pub(super) fn repositories_column<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let body = if app.settings.repositories_pinned {
        container(panel(app, tokens, reading)).width(Length::Fixed(COLUMN_WIDTH))
    } else {
        container(rail(app, tokens, reading)).width(Length::Fixed(RAIL_WIDTH))
    };
    body.height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_secondary),
            border: Border {
                color: tokens.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
        })
        .into()
}

/// The list drawn over the window from the rail, while the column is unpinned.
/// A click anywhere outside it closes it.
pub(super) fn repositories_flyout<'a>(
    app: &'a GitApp,
    top: f32,
    tokens: DesignTokens,
    reading: Reading,
) -> Option<Element<'a, Message>> {
    if app.settings.repositories_pinned || !app.repositories_flyout {
        return None;
    }
    let card = container(panel(app, tokens, reading))
        .width(Length::Fixed(COLUMN_WIDTH))
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(tokens.bg_sidebar)),
            text_color: Some(tokens.text_secondary),
            border: Border {
                color: tokens.accent_primary,
                width: 1.0,
                radius: 0.0.into(),
            },
            shadow: Shadow {
                color: Color { a: 0.5, ..Color::BLACK },
                offset: iced::Vector::new(4.0, 0.0),
                blur_radius: 16.0,
            },
        });
    let outside = mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::ToggleRepositoriesFlyout);
    Some(
        column![
            Space::new(Length::Fill, Length::Fixed(top)),
            row![Space::new(Length::Fixed(RAIL_WIDTH), Length::Fill), card, outside].height(Length::Fill),
        ]
        .into(),
    )
}

fn panel<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let pinned = app.settings.repositories_pinned;
    let small = (reading.size - 2.0).max(10.0);
    let header = row![
        text("REPOSITORIES")
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..reading.font
            })
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        text(app.settings.recent_repositories.len().to_string())
            .font(reading.font)
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        horizontal_space(),
        icon_button(
            Icon::Add,
            "Open a repository…",
            "repositories-open",
            app.tip_ready("repositories-open"),
            Message::ChooseRepositoryFolder,
            !app.opening,
            false,
            tokens,
            reading,
        ),
        icon_button(
            if pinned { Icon::Pinned } else { Icon::Pin },
            if pinned { "Unpin: fold to a rail" } else { "Pin as a column" },
            "repositories-pin",
            app.tip_ready("repositories-pin"),
            Message::ToggleRepositoriesPinned,
            true,
            pinned,
            tokens,
            reading,
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .padding([6, 8]);

    let filter = container(
        row![
            glyph(Icon::Search, reading.size, tokens.text_muted, &reading),
            text_input("Filter repositories", &app.repository_filter)
                .id(text_input::Id::new(REPOSITORY_FILTER_ID))
                .font(reading.font)
                .size(reading.size)
                .padding([3, 4])
                .on_input(Message::RepositoryFilterChanged)
                .style(embedded_field_style(tokens)),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding([0, 6])
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(tokens.bg_app)),
        border: Border {
            color: tokens.chip_border,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    });

    let current = app.sessions.first().map(|session| session.path.as_str());
    let shown = visible_repositories(&app.settings.recent_repositories, &app.repository_filter);
    let mut list = column![].spacing(2).padding([4, 6]);
    if shown.is_empty() {
        list = list.push(
            text(if app.settings.recent_repositories.is_empty() {
                "Repositories you open appear here."
            } else {
                "No repository matches."
            })
            .font(reading.font)
            .size(small)
            .style(move |_| text::Style { color: Some(tokens.text_muted) }),
        );
    }
    for path in shown {
        let is_current = current.is_some_and(|current| same_path(current, path));
        list = list.push(repository_row(app, path, is_current, tokens, reading));
    }

    column![
        header,
        container(filter).padding([0, 8]),
        scrollable(list).height(Length::Fill),
    ]
    .spacing(6)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn repository_row<'a>(
    app: &'a GitApp,
    path: &'a str,
    is_current: bool,
    tokens: DesignTokens,
    reading: Reading,
) -> Element<'a, Message> {
    let small = (reading.size - 2.0).max(10.0);
    let name = basename(path).to_string();
    let parent = parent_folder(path);
    let labels = column![
        one_line(
            text(name.clone())
                .font(if is_current {
                    iced::Font {
                        weight: iced::font::Weight::Semibold,
                        ..reading.font
                    }
                } else {
                    reading.font
                })
                .size(reading.size)
                .wrapping(iced::widget::text::Wrapping::None)
                .style(move |_| text::Style {
                    color: Some(if is_current { tokens.accent_primary } else { tokens.text_primary }),
                }),
            reading.size,
            Length::Fill,
        ),
        one_line(
            text(parent)
                .font(reading.mono)
                .size(small)
                .wrapping(iced::widget::text::Wrapping::None)
                .style(move |_| text::Style { color: Some(tokens.text_muted) }),
            small,
            Length::Fill,
        ),
    ]
    .width(Length::Fill);

    let open = button(
        row![tile(&name, is_current, tokens, reading), labels]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ROW_HEIGHT))
    .padding([2, 6])
    .on_press_maybe((!is_current && !app.opening).then(|| Message::OpenRepository(path.to_string())))
    .style(move |_theme, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if is_current {
                tokens.bg_raised
            } else if hot {
                tokens.bg_raised
            } else {
                Color::TRANSPARENT
            })),
            text_color: tokens.text_primary,
            border: Border {
                color: if is_current { tokens.accent_primary } else { Color::TRANSPARENT },
                width: if is_current { 1.0 } else { 0.0 },
                radius: 4.0.into(),
            },
            shadow: Shadow::default(),
        }
    });

    let mut line = row![open].align_y(Alignment::Center);
    if !is_current {
        line = line.push(
            button(glyph(Icon::Close, small, tokens.text_muted, &reading))
                .padding([2, 4])
                .on_press(Message::ForgetRepository(path.to_string()))
                .style(move |_theme, status| {
                    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(Background::Color(if hot { tokens.border } else { Color::TRANSPARENT })),
                        text_color: tokens.text_muted,
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        shadow: Shadow::default(),
                    }
                }),
        );
    }
    line.into()
}

/// The folded rail: a button that shows the list, then one initial per repository.
fn rail<'a>(app: &'a GitApp, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let current = app.sessions.first().map(|session| session.path.as_str());
    let mut tiles = column![icon_button(
        Icon::Repo,
        "Repositories",
        "repositories-rail",
        app.tip_ready("repositories-rail"),
        Message::ToggleRepositoriesFlyout,
        true,
        app.repositories_flyout,
        tokens,
        reading,
    )]
    .spacing(6)
    .padding([8, 0])
    .align_x(Alignment::Center)
    .width(Length::Fill);
    for path in &app.settings.recent_repositories {
        let is_current = current.is_some_and(|current| same_path(current, path));
        let name = basename(path).to_string();
        tiles = tiles.push(
            button(tile(&name, is_current, tokens, reading))
                .padding(0)
                .on_press_maybe((!is_current && !app.opening).then(|| Message::OpenRepository(path.clone())))
                .style(|_theme, _status| button::Style::default()),
        );
    }
    scrollable(tiles).height(Length::Fill).into()
}

/// A square with the repository's initial. The open one is filled with the accent.
fn tile<'a>(name: &str, is_current: bool, tokens: DesignTokens, reading: Reading) -> Element<'a, Message> {
    let initial = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    container(
        text(initial)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..reading.font
            })
            .size(reading.size)
            .style(move |_| text::Style {
                color: Some(if is_current { Color::BLACK } else { tokens.text_primary }),
            }),
    )
    .width(Length::Fixed(TILE_SIZE))
    .height(Length::Fixed(TILE_SIZE))
    .center_x(Length::Fixed(TILE_SIZE))
    .center_y(Length::Fixed(TILE_SIZE))
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(if is_current { tokens.accent_primary } else { tokens.chip })),
        border: Border {
            color: if is_current { tokens.accent_primary } else { tokens.chip_border },
            width: 1.0,
            radius: 5.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// The folder a repository sits in, for the second line of its row.
fn parent_folder(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    match trimmed.rfind(['/', '\\']) {
        Some(at) => trimmed[..at].to_string(),
        None => String::new(),
    }
}

/// Recent repositories whose path contains every typed term, ignoring case.
pub(super) fn visible_repositories<'a>(paths: &'a [String], filter: &str) -> Vec<&'a str> {
    let terms: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    paths
        .iter()
        .map(String::as_str)
        .filter(|path| {
            let lower = path.to_lowercase();
            terms.iter().all(|term| lower.contains(term.as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_every_term_anywhere_in_the_path() {
        let paths = vec![r"D:\Repos\ISL-Backend".to_string(), r"D:\Repos\GitCthulhu".to_string()];
        assert_eq!(visible_repositories(&paths, ""), vec![r"D:\Repos\ISL-Backend", r"D:\Repos\GitCthulhu"]);
        assert_eq!(visible_repositories(&paths, "repos isl"), vec![r"D:\Repos\ISL-Backend"]);
        assert!(visible_repositories(&paths, "nothing").is_empty());
    }

    #[test]
    fn the_parent_folder_drops_the_last_name() {
        assert_eq!(parent_folder(r"D:\Repos\ISL\ISL-Backend\"), r"D:\Repos\ISL");
        assert_eq!(parent_folder("/home/me/repo"), "/home/me");
        assert_eq!(parent_folder("repo"), "");
    }
}
