//! Icons from the bundled Codicons font (`assets/fonts/codicon.ttf`, CC BY 4.0).
//!
//! Every icon is drawn from this one font, which is compiled into the binary. Before, icons
//! were Unicode symbols looked up in whatever fonts the machine had; Windows lacked several
//! of them and drew empty boxes.

use iced::Font;

/// The font file, loaded once at startup.
pub(crate) const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/codicon.ttf");

/// The family name inside `codicon.ttf`.
pub(crate) const FONT: Font = Font::with_name("codicon");

/// One icon. The codepoints come from Codicons 0.0.45's `dist/codicon.csv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Add,
    ArrowSmallDown,
    ArrowSmallUp,
    Branch,
    Check,
    CheckAll,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    CircleFilled,
    ClearAll,
    Close,
    Cloud,
    Commit,
    Compare,
    Discard,
    Edit,
    Fetch,
    File,
    Folder,
    FolderOpened,
    ListFlat,
    ListTree,
    Merge,
    Pull,
    Push,
    Refresh,
    Remove,
    Search,
    Sparkle,
    StashApply,
    StashPop,
    Stash,
    SplitView,
    SwitchBranch,
    Tag,
    Target,
    Trash,
    Worktree,
}

impl Icon {
    pub(crate) fn codepoint(self) -> char {
        match self {
            Icon::Add => '\u{EA60}',
            Icon::ArrowSmallDown => '\u{EA9D}',
            Icon::ArrowSmallUp => '\u{EAA0}',
            Icon::Branch => '\u{EA68}',
            Icon::Check => '\u{EAB2}',
            Icon::CheckAll => '\u{EBB1}',
            Icon::ChevronDown => '\u{EAB4}',
            Icon::ChevronLeft => '\u{EAB5}',
            Icon::ChevronRight => '\u{EAB6}',
            Icon::CircleFilled => '\u{EA71}',
            Icon::ClearAll => '\u{EABF}',
            Icon::Close => '\u{EA76}',
            Icon::Cloud => '\u{EBAA}',
            Icon::Commit => '\u{EAFC}',
            Icon::Compare => '\u{EAFD}',
            Icon::Discard => '\u{EAE2}',
            Icon::Edit => '\u{EA73}',
            Icon::Fetch => '\u{EC1D}',
            Icon::File => '\u{EA7B}',
            Icon::Folder => '\u{EA83}',
            Icon::FolderOpened => '\u{EAF7}',
            Icon::ListFlat => '\u{EB84}',
            Icon::ListTree => '\u{EB86}',
            Icon::Merge => '\u{EAFE}',
            Icon::Pull => '\u{EB40}',
            Icon::Push => '\u{EB41}',
            Icon::Refresh => '\u{EB37}',
            Icon::Remove => '\u{EB3B}',
            Icon::Search => '\u{EA6D}',
            Icon::Sparkle => '\u{EC10}',
            Icon::StashApply => '\u{EC27}',
            Icon::StashPop => '\u{EC28}',
            Icon::Stash => '\u{EC26}',
            Icon::SplitView => '\u{EB56}',
            Icon::SwitchBranch => '\u{EBCB}',
            Icon::Tag => '\u{EA66}',
            Icon::Target => '\u{EBF8}',
            Icon::Trash => '\u{EA81}',
            Icon::Worktree => '\u{EA63}',
        }
    }

    /// Fold chevron: right when closed, down when open.
    pub(crate) fn fold(open: bool) -> Icon {
        if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every codepoint must be a glyph in the bundled font, or the icon draws as a box.
    #[test]
    fn every_icon_is_in_the_font() {
        let icons = [
            Icon::Add,
            Icon::ArrowSmallDown, Icon::ArrowSmallUp, Icon::SplitView, Icon::Branch, Icon::Check, Icon::CheckAll, Icon::ChevronDown,
            Icon::ChevronLeft, Icon::ChevronRight, Icon::CircleFilled, Icon::ClearAll, Icon::Close, Icon::Cloud,
            Icon::Commit, Icon::Compare, Icon::Discard, Icon::Edit, Icon::Fetch,
            Icon::File, Icon::Folder, Icon::FolderOpened, Icon::ListFlat, Icon::ListTree,
            Icon::Merge, Icon::Pull, Icon::Push, Icon::Refresh, Icon::Remove, Icon::Search,
            Icon::Sparkle, Icon::StashApply, Icon::StashPop, Icon::Stash, Icon::SwitchBranch,
            Icon::Tag, Icon::Target, Icon::Trash, Icon::Worktree,
        ];
        let covered = cmap_codepoints(FONT_BYTES);
        for icon in icons {
            assert!(covered.contains(&(icon.codepoint() as u32)), "{icon:?} is not in codicon.ttf");
        }
    }

    /// Codepoints of the font's format-4 `cmap` subtable. Enough for a BMP icon font.
    fn cmap_codepoints(font: &[u8]) -> Vec<u32> {
        let u16_at = |at: usize| u16::from_be_bytes([font[at], font[at + 1]]) as usize;
        let u32_at = |at: usize| u32::from_be_bytes([font[at], font[at + 1], font[at + 2], font[at + 3]]) as usize;
        let tables = u16_at(4);
        let cmap = (0..tables)
            .map(|index| 12 + index * 16)
            .find(|&record| &font[record..record + 4] == b"cmap")
            .map(|record| u32_at(record + 8))
            .expect("cmap table");
        let subtables = u16_at(cmap + 2);
        let format4 = (0..subtables)
            .map(|index| cmap + u32_at(cmap + 4 + index * 8 + 4))
            .find(|&table| u16_at(table) == 4)
            .expect("format 4 cmap");
        let segments = u16_at(format4 + 6) / 2;
        let ends = format4 + 14;
        let starts = ends + segments * 2 + 2;
        let mut points = Vec::new();
        for segment in 0..segments {
            let (start, end) = (u16_at(starts + segment * 2), u16_at(ends + segment * 2));
            if start != 0xFFFF {
                points.extend((start..=end).map(|point| point as u32));
            }
        }
        points
    }
}
