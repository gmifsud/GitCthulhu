use iced::Color;

/// Tint behind an added or removed diff line. The wireframe uses 12%.
const DIFF_TINT: f32 = 0.12;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DesignTokens {
    pub bg_app: Color,
    pub bg_sidebar: Color,
    pub bg_chrome: Color,
    /// Selected-row fill. The graph and the diff paint it in later phases.
    #[allow(dead_code)]
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
    /// Row tint behind an added or removed line. The diff column paints it later.
    #[allow(dead_code)]
    pub added_bg: Color,
    /// Row tint behind a removed line. The diff column paints it later.
    #[allow(dead_code)]
    pub removed_bg: Color,
}

impl DesignTokens {
    pub(crate) fn deep_focus() -> Self {
        Self {
            bg_app: hex_color(0x0D0D0F),
            bg_sidebar: hex_color(0x111214),
            bg_chrome: hex_color(0x151619),
            bg_raised: hex_color(0x1A1B1E),
            text_primary: hex_color(0xFFFFFF),
            text_secondary: hex_color(0xD1D1D1),
            text_muted: hex_color(0x5A5A5A),
            border: hex_color(0x2D2D30),
            accent_primary: hex_color(0xF27D26),
            chip: hex_color(0x232429),
            chip_border: hex_color(0x3A3B40),
            added: hex_color(0x7DCEA0),
            removed: hex_color(0xE07A7A),
            added_bg: hex_color_alpha(0x7DCEA0, DIFF_TINT),
            removed_bg: hex_color_alpha(0xE07A7A, DIFF_TINT),
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

fn hex_color_alpha(hex: u32, alpha: f32) -> Color {
    Color {
        a: alpha,
        ..hex_color(hex)
    }
}
