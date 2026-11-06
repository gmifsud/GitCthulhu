//! Builds the design system's files for `/design-sync` (`design_sync` crate).
//!
//! The authored files under `design/system/` — brand book, layout and icon sections, usage notes,
//! component cards — are copied through unchanged. Everything the code owns is stamped from the
//! crate itself so the published system cannot drift from what the window draws: every colour in
//! [`DesignTokens`], the graph lanes, the icon font and its codepoints, and the marks. The result
//! lands under `<out>/project/`, laid out exactly as the Design System artifact serves it.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use iced::Color;
use serde_json::{json, Map, Value};

use crate::commit_graph::LANE_COLOURS;
use crate::icons::{self, Icon};
use crate::theme::{hex_color, DesignTokens};

pub const SYSTEM_TITLE: &str = "GitCthulhu Design System";
pub const NAMESPACE: &str = "GitCthulhu";
/// The one theme, as `tokens.json` names it.
pub const THEME_ID: &str = "deep-focus";
/// Where the authored files live, under the repository root.
pub const AUTHORED_DIR: &str = "design/system";
/// The marks copied into `assets/Logos`, under the repository root.
const APP_ICON: &str = "app/assets/git_cthulhu.ico";
const SOURCE_ART: &str = "app/assets/cthulhu.jpg";
const WINDOW_ICON_RGBA: &[u8] = include_bytes!("../assets/icon-64.rgba");
const WINDOW_ICON_SIZE: u32 = 64;

/// What a sync changed against the authored `tokens.json`.
#[derive(Debug, Default)]
pub struct Report {
    /// `name: old -> new` for every colour whose value the code moved.
    pub changed: Vec<String>,
    /// Colours the code defines that the authored file lacked; added with an empty usage note.
    pub added: Vec<String>,
    /// Colours in the authored file that no code value backs (literals in the views): left as written.
    pub authored_only: Vec<String>,
    /// Files written under `<out>/project/`.
    pub written: Vec<String>,
}

/// Build the system into `out`. `git_ref` is recorded as provenance (`main@1a2b3c4`); `by` names
/// the person the sync runs for.
pub fn sync(repo_root: &Path, out: &Path, git_ref: &str, by: &str) -> io::Result<Report> {
    let authored = repo_root.join(AUTHORED_DIR);
    let project = out.join("project");
    if project.exists() {
        fs::remove_dir_all(&project)?;
    }
    fs::create_dir_all(&project)?;
    let mut report = Report::default();

    copy_tree(&authored, &project, &mut report.written)?;

    let tokens_path = project.join("tokens.json");
    let mut tokens: Value = serde_json::from_slice(&fs::read(&tokens_path)?).map_err(invalid)?;
    stamp_colours(&mut tokens, &mut report)?;
    let now = rfc3339_now();
    let meta = tokens
        .get_mut("meta")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| invalid("tokens.json has no meta object"))?;
    meta.insert("ref".into(), Value::String(git_ref.to_string()));
    meta.insert("synced".into(), Value::String(now[..10].to_string()));
    write(&project, "tokens.json", serde_json::to_string_pretty(&tokens).map_err(invalid)?, &mut report)?;

    check_icon_table(&fs::read_to_string(project.join("icons.md"))?)?;

    write(&project, "tokens.css", tokens_css(&tokens)?, &mut report)?;
    write(&project, "fonts/codicon.ttf", icons::FONT_BYTES, &mut report)?;

    let app_icon = png::largest_png_frame(&fs::read(repo_root.join(APP_ICON))?)?;
    write(&project, "assets/Logos/git-cthulhu-icon.png", app_icon, &mut report)?;
    let window_icon = png::encode_rgba(WINDOW_ICON_RGBA, WINDOW_ICON_SIZE, WINDOW_ICON_SIZE)?;
    write(&project, "assets/Logos/git-cthulhu-window-icon-64.png", window_icon, &mut report)?;
    write(&project, "assets/Logos/cthulhu.jpg", fs::read(repo_root.join(SOURCE_ART))?, &mut report)?;

    let index = index(&project, git_ref, by, &now)?;
    write(&project, "design-system.json", serde_json::to_string_pretty(&index).map_err(invalid)?, &mut report)?;
    Ok(report)
}

/// Every colour the code defines, by the name `tokens.json` uses for it.
///
/// Scrims, the popup shadow and the like are literals in the views, not fields here: they stay
/// authored and show up in [`Report::authored_only`].
pub fn code_colours() -> Vec<(String, Color)> {
    let t = DesignTokens::deep_focus();
    let mut colours: Vec<(String, Color)> = [
        ("bg-app", t.bg_app),
        ("bg-sidebar", t.bg_sidebar),
        ("bg-chrome", t.bg_chrome),
        ("bg-raised", t.bg_raised),
        ("text-primary", t.text_primary),
        ("text-secondary", t.text_secondary),
        ("text-muted", t.text_muted),
        ("border", t.border),
        ("accent-primary", t.accent_primary),
        // Text on an accent fill is `Color::BLACK` wherever the views paint it.
        ("on-accent", Color::BLACK),
        ("chip", t.chip),
        ("chip-border", t.chip_border),
        ("added", t.added),
        ("removed", t.removed),
        ("added-bg", t.added_bg),
        ("removed-bg", t.removed_bg),
        ("modified-bg", t.modified_bg),
        ("modified-word-bg", t.modified_word_bg),
        ("added-word-bg", t.added_word_bg),
        ("removed-word-bg", t.removed_word_bg),
        ("vcs-modified", t.vcs_modified),
        ("vcs-added", t.vcs_added),
        ("vcs-deleted", t.vcs_deleted),
        ("vcs-untracked", t.vcs_untracked),
        ("vcs-renamed", t.vcs_renamed),
        ("vcs-conflicted", t.vcs_conflicted),
        ("incoming", t.incoming),
        ("outgoing", t.outgoing),
    ]
    .into_iter()
    .map(|(name, colour)| (name.to_string(), colour))
    .collect();
    for (lane, hex) in LANE_COLOURS.iter().enumerate() {
        colours.push((format!("lane-{lane}"), hex_color(*hex)));
    }
    colours
}

/// Overwrite every code-owned colour's value in `tokens`, keeping the authored usage notes.
fn stamp_colours(tokens: &mut Value, report: &mut Report) -> io::Result<()> {
    let list = tokens
        .pointer_mut("/color/tokens")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("tokens.json has no color.tokens list"))?;
    let code = code_colours();
    for (name, colour) in &code {
        let value = hex(*colour);
        match list.iter_mut().find(|entry| entry.get("name").and_then(Value::as_str) == Some(name)) {
            Some(entry) => {
                let old = entry.get("value").and_then(Value::as_str).unwrap_or("").to_string();
                if old != value {
                    report.changed.push(format!("{name}: {old} -> {value}"));
                    entry["value"] = Value::String(value);
                }
            }
            None => {
                report.added.push(name.clone());
                list.push(json!({ "name": name, "value": value, "usage": "" }));
            }
        }
    }
    for entry in list.iter() {
        if let Some(name) = entry.get("name").and_then(Value::as_str) {
            if !code.iter().any(|(code_name, _)| code_name == name) {
                report.authored_only.push(name.to_string());
            }
        }
    }
    Ok(())
}

/// `#rrggbb`, or `#rrggbbaa` when the colour is translucent, lowercased as the page wants it.
pub fn hex(colour: Color) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut out = format!("#{:02x}{:02x}{:02x}", channel(colour.r), channel(colour.g), channel(colour.b));
    if colour.a < 1.0 {
        let _ = write!(out, "{:02x}", channel(colour.a));
    }
    out
}

/// The icon name `icons.md` lists: the variant in kebab case (`ArrowSmallDown` → `arrow-small-down`).
pub(crate) fn icon_name(icon: Icon) -> String {
    let mut name = String::new();
    for (index, ch) in format!("{icon:?}").chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            name.push('-');
        }
        name.push(ch.to_ascii_lowercase());
    }
    name
}

/// `icons.md` is authored (its "where" column is prose), so a sync checks it against the enum
/// instead of regenerating it: every variant has a row with its codepoint, and no row is stale.
fn check_icon_table(markdown: &str) -> io::Result<()> {
    let mut rows: Vec<(String, u32)> = Vec::new();
    for line in markdown.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 4 {
            continue;
        }
        if let Some(point) = cells[2].strip_prefix("U+").and_then(|digits| u32::from_str_radix(digits, 16).ok()) {
            rows.push((cells[1].to_string(), point));
        }
    }
    let mut problems = Vec::new();
    for icon in Icon::ALL {
        let name = icon_name(icon);
        match rows.iter().find(|(row, _)| *row == name) {
            Some((_, point)) if *point == icon.codepoint() as u32 => {}
            Some((_, point)) => problems.push(format!("{name}: icons.md says U+{point:04X}, the code U+{:04X}", icon.codepoint() as u32)),
            None => problems.push(format!("{name}: no row in icons.md")),
        }
    }
    for (row, _) in &rows {
        if !Icon::ALL.iter().any(|icon| icon_name(*icon) == *row) {
            problems.push(format!("{row}: in icons.md but not in the Icon enum"));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(invalid(format!("icons.md is out of step with icons.rs:\n  {}", problems.join("\n  "))))
    }
}

/// `tokens.css` as the Design System page compiles it, so a page that opens a synced system
/// has the same stylesheet it would have written itself.
fn tokens_css(tokens: &Value) -> io::Result<String> {
    let entries = |family: &str| -> Vec<(String, String)> {
        tokens
            .pointer(&format!("/{family}/tokens"))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|entry| {
                        let name = entry.get("name")?.as_str()?;
                        let value = match entry.get("value")? {
                            Value::String(text) => text.clone(),
                            Value::Object(themes) => themes.get(THEME_ID)?.as_str()?.to_string(),
                            _ => return None,
                        };
                        Some((name.to_string(), value))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut css = format!("/* {SYSTEM_TITLE} — generated from tokens.json */\n");
    let _ = writeln!(css, ":root, [data-theme=\"{THEME_ID}\"] {{");
    for (name, value) in entries("color").into_iter().chain(entries("shadow")) {
        let _ = writeln!(css, "  --{name}: {value};");
    }
    css.push_str("}\n:root {\n");
    let families = tokens
        .as_object()
        .map(|top| top.keys().filter(|key| !matches!(key.as_str(), "name" | "version" | "meta" | "color" | "shadow" | "type")).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for family in families {
        for (name, value) in entries(&family) {
            let _ = writeln!(css, "  --{name}: {value};");
        }
    }
    if let Some(stacks) = tokens.pointer("/type/families").and_then(Value::as_object) {
        for (key, stack) in stacks {
            if let Some(stack) = stack.as_str() {
                let _ = writeln!(css, "  --font-{key}: {stack};");
            }
        }
    }
    css.push_str("}\n");
    if let Some(groups) = tokens.pointer("/type/groups").and_then(Value::as_array) {
        for group in groups {
            let family = group.get("family").and_then(Value::as_str).unwrap_or("sans");
            for style in group.get("styles").and_then(Value::as_array).into_iter().flatten() {
                let Some(name) = style.get("name").and_then(Value::as_str) else { continue };
                let family = style.get("family").and_then(Value::as_str).unwrap_or(family);
                let _ = write!(css, ".{name} {{ font-family: var(--font-{family});");
                for (key, property) in [("fontSize", "font-size"), ("lineHeight", "line-height"), ("fontWeight", "font-weight"), ("letterSpacing", "letter-spacing"), ("fontStyle", "font-style")] {
                    match style.get(key) {
                        Some(Value::String(text)) => { let _ = write!(css, " {property}: {text};"); }
                        Some(Value::Number(number)) => { let _ = write!(css, " {property}: {number};"); }
                        _ => {}
                    }
                }
                css.push_str(" }\n");
            }
        }
    }
    if let Some(fonts) = tokens.pointer("/type/fonts").and_then(Value::as_array) {
        for font in fonts {
            let (Some(family), Some(file)) = (font.get("family").and_then(Value::as_str), font.get("file").and_then(Value::as_str)) else { continue };
            let weight = font.get("weight").and_then(Value::as_str).unwrap_or("400");
            let style = font.get("style").and_then(Value::as_str).unwrap_or("normal");
            let _ = writeln!(css, "@font-face {{ font-family: \"{family}\"; src: url(\"{file}\"); font-weight: {weight}; font-style: {style}; }}");
        }
    }
    Ok(css)
}

/// The index the artifact reads: `editing: code` marks the page read-only, since each sync
/// replaces the files. Asset records carry no `blob` yet; `/design-sync` fills them in after the
/// uploads, and keeps the live index's `createdOnFiles`.
fn index(project: &Path, git_ref: &str, by: &str, now: &str) -> io::Result<Value> {
    let logos = project.join("assets/Logos");
    let mut order = Vec::new();
    let mut files = Map::new();
    for (name, media_type) in [
        ("git-cthulhu-icon.png", "image/png"),
        ("git-cthulhu-window-icon-64.png", "image/png"),
        ("cthulhu.jpg", "image/jpeg"),
    ] {
        let size = fs::metadata(logos.join(name))?.len();
        order.push(Value::String(name.to_string()));
        files.insert(name.to_string(), json!({ "name": name, "size": size, "type": media_type }));
    }
    Ok(json!({
        "v": 3,
        "layout": "files",
        "editing": "code",
        "title": SYSTEM_TITLE,
        "namespace": NAMESPACE,
        "libraries": [],
        "sections": {},
        "groups": ["Logos"],
        "assetGroups": { "Logos": { "name": "Logos", "tile": "l", "order": order, "files": files } },
        "blobs": {},
        "docs": { "readme": "project/README.md", "sections": [] },
        "lastChange": {
            "by": by,
            "at": now,
            "via": format!("Claude Code · /design-sync · {git_ref}"),
            "note": "Rebuilt from gui_app by design_sync"
        }
    }))
}

fn copy_tree(from: &Path, to: &Path, written: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&target)?;
            copy_tree(&entry.path(), &target, written)?;
        } else {
            fs::copy(entry.path(), &target)?;
            written.push(target.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn write(project: &Path, relative: &str, bytes: impl AsRef<[u8]>, report: &mut Report) -> io::Result<()> {
    let path = project.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, bytes)?;
    report.written.push(relative.to_string());
    Ok(())
}

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// Now, as `YYYY-MM-DDTHH:MM:SSZ`, without a date crate (Howard Hinnant's civil-from-days).
fn rfc3339_now() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let rest = seconds % 86_400;
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3_600, rest % 3_600 / 60, rest % 60)
}

/// Enough PNG to carry the marks: pick the PNG frame out of an `.ico`, and wrap raw RGBA
/// pixels as an uncompressed (stored-deflate) PNG. No image crate for two small files.
mod png {
    use std::io;

    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    /// The widest PNG-compressed frame of an ICO file, verbatim.
    pub fn largest_png_frame(ico: &[u8]) -> io::Result<Vec<u8>> {
        let u16_at = |at: usize| ico.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
        let u32_at = |at: usize| ico.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let count = u16_at(4).ok_or_else(|| super::invalid("not an ICO file"))?;
        let mut best: Option<(usize, &[u8])> = None;
        for index in 0..count {
            let entry = 6 + index * 16;
            let width = match ico.get(entry) { Some(0) => 256, Some(w) => *w as usize, None => break };
            let (Some(size), Some(offset)) = (u32_at(entry + 8), u32_at(entry + 12)) else { break };
            let Some(frame) = ico.get(offset..offset + size) else { continue };
            if frame.starts_with(&SIGNATURE) && best.is_none_or(|(w, _)| width > w) {
                best = Some((width, frame));
            }
        }
        best.map(|(_, frame)| frame.to_vec()).ok_or_else(|| super::invalid("the ICO has no PNG frame"))
    }

    /// RGBA8 pixels → PNG, with the pixel data in stored deflate blocks.
    pub fn encode_rgba(rgba: &[u8], width: u32, height: u32) -> io::Result<Vec<u8>> {
        let row = width as usize * 4;
        if rgba.len() != row * height as usize {
            return Err(super::invalid("RGBA length does not match the size"));
        }
        let mut raw = Vec::with_capacity((row + 1) * height as usize);
        for line in rgba.chunks(row) {
            raw.push(0); // filter: none
            raw.extend_from_slice(line);
        }
        let mut idat = vec![0x78, 0x01]; // zlib header: deflate, no compression preset
        let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (index, block) in blocks.iter().enumerate() {
            let last = index + 1 == blocks.len();
            idat.push(u8::from(last));
            let len = block.len() as u16;
            idat.extend_from_slice(&len.to_le_bytes());
            idat.extend_from_slice(&(!len).to_le_bytes());
            idat.extend_from_slice(block);
        }
        idat.extend_from_slice(&adler32(&raw).to_be_bytes());

        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, deflate, no filter, no interlace

        let mut png = SIGNATURE.to_vec();
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &idat);
        chunk(&mut png, b"IEND", &[]);
        Ok(png)
    }

    fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    fn adler32(bytes: &[u8]) -> u32 {
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in bytes {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        (b << 16) | a
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_png_frame_is_picked_by_width_and_returned_verbatim() {
            let small = [SIGNATURE.to_vec(), vec![1, 2, 3]].concat();
            let big = [SIGNATURE.to_vec(), vec![9, 9]].concat();
            let mut ico = vec![0, 0, 1, 0, 2, 0];
            let first = 6 + 2 * 16;
            ico.extend_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0]);
            ico.extend_from_slice(&(small.len() as u32).to_le_bytes());
            ico.extend_from_slice(&(first as u32).to_le_bytes());
            ico.extend_from_slice(&[0, 0, 0, 0, 1, 0, 32, 0]);
            ico.extend_from_slice(&(big.len() as u32).to_le_bytes());
            ico.extend_from_slice(&((first + small.len()) as u32).to_le_bytes());
            ico.extend_from_slice(&small);
            ico.extend_from_slice(&big);
            assert_eq!(largest_png_frame(&ico).unwrap(), big);
        }

        #[test]
        fn encoded_png_has_the_signature_header_and_trailer() {
            let png = encode_rgba(&[255, 0, 0, 255, 0, 255, 0, 255], 2, 1).unwrap();
            assert!(png.starts_with(&SIGNATURE));
            assert_eq!(&png[12..16], b"IHDR");
            assert_eq!(u32::from_be_bytes([png[16], png[17], png[18], png[19]]), 2);
            assert!(png.ends_with(&[b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        }

        #[test]
        fn crc_and_adler_match_the_reference_vectors() {
            assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
            assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_struct_colour_is_exported_once() {
        let colours = code_colours();
        let mut names: Vec<&str> = colours.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), colours.len());
        // 27 DesignTokens fields + on-accent + 6 lanes.
        assert_eq!(colours.len(), 34);
    }

    #[test]
    fn hex_round_trips_hex_color_and_marks_alpha() {
        assert_eq!(hex(hex_color(0xF27D26)), "#f27d26");
        assert_eq!(hex(Color { a: 0.72, ..hex_color(0x0D0D0F) }), "#0d0d0fb8");
    }

    #[test]
    fn icon_names_are_kebab_case() {
        assert_eq!(icon_name(Icon::ArrowSmallDown), "arrow-small-down");
        assert_eq!(icon_name(Icon::Tag), "tag");
    }

    #[test]
    fn the_authored_icon_table_matches_the_enum() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(AUTHORED_DIR).join("icons.md");
        check_icon_table(&fs::read_to_string(path).expect("design/system/icons.md")).expect("icons.md agrees with icons.rs");
    }
}
