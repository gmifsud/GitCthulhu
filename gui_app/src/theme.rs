use iced::Color;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DesignTokens {
    pub bg_app: Color,
    pub bg_sidebar: Color,
    pub bg_chrome: Color,
    pub bg_raised: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub border: Color,
    pub accent_primary: Color,
    pub chip: Color,
    pub chip_border: Color,
    pub added: Color,
    pub removed: Color,
    /// Solid dark wash behind added lines (opaque — not a translucent overlay).
    pub added_bg: Color,
    /// Solid dark wash behind removed lines.
    pub removed_bg: Color,
    /// File-status ink, after the JetBrains changes view: modified, added, deleted,
    /// unversioned, renamed, conflicted.
    pub vcs_modified: Color,
    pub vcs_added: Color,
    pub vcs_deleted: Color,
    pub vcs_untracked: Color,
    pub vcs_renamed: Color,
    pub vcs_conflicted: Color,
    /// Wash behind a changed line pair in the side-by-side diff (modified, not added or removed).
    pub modified_bg: Color,
    /// Stronger wash behind the changed words inside a modified line.
    pub modified_word_bg: Color,
    pub added_word_bg: Color,
    pub removed_word_bg: Color,
}

impl DesignTokens {
    /// Deep charcoal shell, ember accent — tuned for a local Git client that
    /// spends most of its life on history and staging, not marketing chrome.
    pub(crate) fn deep_focus() -> Self {
        Self {
            bg_app: hex_color(0x0D0D0F),
            bg_sidebar: hex_color(0x111214),
            bg_chrome: hex_color(0x151619),
            bg_raised: hex_color(0x1A1B1E),
            text_primary: hex_color(0xF4F4F5),
            text_secondary: hex_color(0xC8C8CC),
            // Wireframe `--quiet`: scannable labels and glyphs, not wallpaper grey.
            text_muted: hex_color(0x8A8A8A),
            border: hex_color(0x2D2D30),
            accent_primary: hex_color(0xF27D26),
            chip: hex_color(0x232429),
            chip_border: hex_color(0x3A3B40),
            // Diff ink: bright enough to read on the solid washes below.
            added: hex_color(0x7EE787),
            removed: hex_color(0xFFA198),
            added_bg: hex_color(0x163227),
            removed_bg: hex_color(0x3B1719),
            vcs_modified: hex_color(0x70AEFF),
            vcs_added: hex_color(0x73BD79),
            vcs_deleted: hex_color(0x8C8C8C),
            vcs_untracked: hex_color(0xD1675A),
            vcs_renamed: hex_color(0x6BC1C1),
            vcs_conflicted: hex_color(0xF75464),
            modified_bg: hex_color(0x1C2A3F),
            modified_word_bg: hex_color(0x2C4A73),
            added_word_bg: hex_color(0x23593A),
            removed_word_bg: hex_color(0x6B2A2C),
        }
    }
}

pub(crate) fn hex_color(hex: u32) -> Color {
    Color::from_rgb(
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
    )
}

impl DesignTokens {
    /// Ink for a changed file's name.
    pub(crate) fn change_ink(&self, kind: core_domain::ChangeKind) -> Color {
        use core_domain::ChangeKind;
        match kind {
            ChangeKind::Added => self.vcs_added,
            ChangeKind::Modified => self.vcs_modified,
            ChangeKind::Deleted => self.vcs_deleted,
            ChangeKind::Renamed => self.vcs_renamed,
            ChangeKind::Conflicted => self.vcs_conflicted,
            ChangeKind::Untracked => self.vcs_untracked,
        }
    }
}
