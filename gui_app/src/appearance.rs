//! Font faces and sizes the Appearance section is allowed to store.

use core_domain::AppSettings;

pub(crate) const UI_FONTS: [&str; 3] = ["Segoe UI", "Familjen Grotesk", "Fragment Mono"];

pub(crate) fn font_family(name: &str) -> &'static str {
    UI_FONTS
        .iter()
        .copied()
        .find(|font| *font == name)
        .unwrap_or(AppSettings::DEFAULT_UI_FONT)
}

pub(crate) fn font_size(size: u8) -> u8 {
    size.clamp(AppSettings::MIN_UI_SIZE, AppSettings::MAX_UI_SIZE)
}
