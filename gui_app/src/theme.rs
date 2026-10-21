use iced::Color;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DesignTokens {
    pub bg_app: Color,
    pub bg_sidebar: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub border: Color,
    pub accent_primary: Color,
}

impl DesignTokens {
    pub(crate) fn deep_focus() -> Self {
        Self {
            bg_app: hex_color(0x0D0D0F),
            bg_sidebar: hex_color(0x111214),
            text_primary: hex_color(0xFFFFFF),
            text_secondary: hex_color(0xD1D1D1),
            text_muted: hex_color(0x5A5A5A),
            border: hex_color(0x2D2D30),
            accent_primary: hex_color(0xF27D26),
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
