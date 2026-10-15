//! CJK font discovery.
//!
//! The entire UI is in Chinese, so a font with CJK coverage is mandatory.  The
//! default egui fonts cover Latin only, and bundling a full CJK face would add
//! ~10 MB to every binary.  Instead we locate a system font at startup: first
//! from a list of well-known paths (fast, and covers almost every real desktop)
//! and then by asking `fontdb` for any installed family with a CJK name.
//!
//! If nothing is found the app still runs; Chinese text simply renders as
//! tofu, and the UI says so in the log so the cause is obvious.

use std::sync::Arc;

use eframe::egui;

use crate::settings::FontSettings;

/// Family names that indicate CJK coverage, in descending preference.
///
/// Ordering matters: a sans-serif with broad coverage is a better UI font than
/// a serif or a handwriting face.
const PREFERRED_FAMILIES: &[&str] = &[
    "Noto Sans CJK SC",
    "Noto Sans SC",
    "Source Han Sans SC",
    "Sarasa Gothic SC",
    "Sarasa Mono SC",
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "PingFang SC",
    "Hiragino Sans GB",
    "Heiti SC",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
    "Noto Sans CJK TC",
    "Noto Sans CJK JP",
    "SimHei",
    "SimSun",
    "Arial Unicode MS",
];

/// Well-known font file paths, tried before the slower database scan.
#[cfg(target_os = "linux")]
const KNOWN_PATHS: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansSC-Regular.otf",
    "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.ttf",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/arphic/uming.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
];

#[cfg(target_os = "windows")]
const KNOWN_PATHS: &[&str] = &[
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\msyh.ttf",
    "C:\\Windows\\Fonts\\msyhbd.ttc",
    "C:\\Windows\\Fonts\\simhei.ttf",
    "C:\\Windows\\Fonts\\simsun.ttc",
    "C:\\Windows\\Fonts\\Deng.ttf",
];

#[cfg(target_os = "macos")]
const KNOWN_PATHS: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/STHeiti Medium.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
];

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const KNOWN_PATHS: &[&str] = &[];

/// A loaded font face, ready to hand to egui.
#[derive(Clone)]
pub struct LoadedFont {
    /// Family name, for display in the UI.
    pub family: String,
    pub data: Vec<u8>,
    /// Face index within a font collection (`.ttc`).
    pub index: u32,
}

/// Stable egui family names for the two independently selectable text areas.
///
/// egui supports named font families, which lets the receive and send editors
/// use different faces without changing the interface font.
pub const RECEIVE_FONT_FAMILY: &str = "xcom-receive";
pub const SEND_FONT_FAMILY: &str = "xcom-send";

/// Well-known UI font paths (Zed / IBM Plex / clean sans).
#[cfg(target_os = "linux")]
const KNOWN_UI_PATHS: &[&str] = &[
    "/usr/share/fonts/windows/ZedMonoNerdFontPropo-Regular.ttf",
    "/usr/share/fonts/truetype/ibm-plex/IBMPlexSans-Regular.ttf",
    "/usr/share/fonts/opentype/ibm-plex/IBMPlexSans-Regular.otf",
    "/usr/share/fonts/windows/ZedMonoNerdFont-Regular.ttf",
];

#[cfg(not(target_os = "linux"))]
const KNOWN_UI_PATHS: &[&str] = &[];

/// Well-known Monospace font paths (Zed Mono / IBM Plex Mono).
#[cfg(target_os = "linux")]
const KNOWN_MONO_PATHS: &[&str] = &[
    "/usr/share/fonts/windows/ZedMonoNerdFontMono-Regular.ttf",
    "/usr/share/fonts/windows/ZedMonoNerdFont-Regular.ttf",
    "/usr/share/fonts/truetype/ibm-plex/IBMPlexMono-Regular.ttf",
    "/usr/share/fonts/opentype/ibm-plex/IBMPlexMono-Regular.otf",
];

#[cfg(not(target_os = "linux"))]
const KNOWN_MONO_PATHS: &[&str] = &[];

/// UI font family preference list.
const PREFERRED_UI_FAMILIES: &[&str] = &[
    "IBM Plex Sans",
    "ZedMono Nerd Font Propo",
    "ZedMono NFP",
    "ZedMono Nerd Font",
    "Inter",
    "Cantarell",
    "Noto Sans",
    "Segoe UI",
];

/// Monospace font family preference list.
const PREFERRED_MONO_FAMILIES: &[&str] = &[
    "Noto Sans Mono CJK SC",
    "Sarasa Mono SC",
    "Source Han Mono SC",
    "ZedMono Nerd Font Mono",
    "ZedMono NFM",
    "ZedMono Nerd Font",
    "IBM Plex Mono",
    "JetBrains Mono",
    "DejaVu Sans Mono",
];

/// All installed family names, sorted for the settings combo boxes.
pub fn available_families() -> Vec<String> {
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            let mut names = std::collections::BTreeSet::new();
            for face in db.faces() {
                for (name, _) in &face.families {
                    names.insert(name.clone());
                }
            }
            names.into_iter().collect()
        })
        .clone()
}

/// Load an exact family chosen by the user.
pub fn find_family(name: &str) -> Option<LoadedFont> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let face = db.faces().find(|face| {
        face.families
            .iter()
            .any(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
    })?;
    let data = db.with_face_data(face.id, |bytes, _| bytes.to_vec())?;
    if data.is_empty() {
        return None;
    }
    Some(LoadedFont {
        family: name.to_owned(),
        data,
        index: face.index,
    })
}

/// Try to find a Zed-like UI font.
pub fn find_ui_font() -> Option<LoadedFont> {
    for path in KNOWN_UI_PATHS {
        if let Ok(data) = std::fs::read(path)
            && !data.is_empty()
        {
            return Some(LoadedFont {
                family: file_stem(path),
                data,
                index: 0,
            });
        }
    }
    scan_families(PREFERRED_UI_FAMILIES)
}

/// Try to find a Zed-like Monospace font.
pub fn find_mono_font() -> Option<LoadedFont> {
    // Prefer a CJK-capable monospace face: using one face for Latin and CJK
    // keeps mixed-script receive/send text on the same baseline.
    if let Some(font) = scan_families(PREFERRED_MONO_FAMILIES) {
        return Some(font);
    }
    for path in KNOWN_MONO_PATHS {
        if let Ok(data) = std::fs::read(path)
            && !data.is_empty()
        {
            return Some(LoadedFont {
                family: file_stem(path),
                data,
                index: 0,
            });
        }
    }
    None
}

/// Family name of the UI font installed at startup.
static INSTALLED_UI_FAMILY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Family name of the UI font in use.
pub fn installed_ui_family() -> Option<String> {
    INSTALLED_UI_FAMILY.get().cloned().flatten()
}

/// Try to find a CJK-capable system font.
pub fn find_cjk_font() -> Option<LoadedFont> {
    for path in KNOWN_PATHS {
        if let Ok(data) = std::fs::read(path)
            && !data.is_empty()
        {
            return Some(LoadedFont {
                family: file_stem(path),
                data,
                index: 0,
            });
        }
    }
    scan_families(PREFERRED_FAMILIES)
}

/// Ask `fontdb` for an installed family from `preferred`.
fn scan_families(preferred: &[&str]) -> Option<LoadedFont> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    for wanted in preferred {
        let family = db
            .faces()
            .find(|face| {
                face.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(wanted))
            })
            .map(|face| (face.id, face.index, (*wanted).to_owned()));
        let Some((id, index, name)) = family else {
            continue;
        };
        let data = db.with_face_data(id, |bytes, _| bytes.to_vec())?;
        if data.is_empty() {
            continue;
        }
        return Some(LoadedFont {
            family: name,
            data,
            index,
        });
    }
    None
}

/// Extract the file name without its extension.
///
/// Both separators are handled explicitly rather than deferring to
/// `Path::file_stem`, so the Windows path list produces sensible names even
/// when this code is compiled and tested on Unix.
fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        _ => name.to_owned(),
    }
}

/// The result of installing fonts into an egui context.
pub enum FontStatus {
    /// A CJK font was installed.
    Loaded(String),
    /// No CJK font was found; Chinese text will not render.
    Missing,
}

/// Family name of the font installed at startup.
///
/// Cached rather than re-discovered: the lookup reads a multi-megabyte font
/// file, so it must happen exactly once.
static INSTALLED_FAMILY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Family name of the CJK font in use, or `None` when none was found.
pub fn installed_family() -> Option<String> {
    INSTALLED_FAMILY.get().cloned().flatten()
}

fn add_font(fonts: &mut egui::FontDefinitions, key: &str, font: &LoadedFont) {
    let mut data = egui::FontData::from_owned(font.data.clone());
    data.index = font.index;
    fonts.font_data.insert(key.to_owned(), Arc::new(data));
    fonts
        .families
        .entry(egui::FontFamily::Name(Arc::from(key)))
        .or_default()
        .insert(0, key.to_owned());
}

fn bind_font_key(fonts: &mut egui::FontDefinitions, family: &egui::FontFamily, key: &str) {
    fonts
        .families
        .entry(family.clone())
        .or_default()
        .insert(0, key.to_owned());
}

fn bind_font_keys(fonts: &mut egui::FontDefinitions, family: &egui::FontFamily, keys: &[String]) {
    let list = fonts.families.entry(family.clone()).or_default();
    for key in keys {
        list.insert(0, key.clone());
    }
}

fn push_cjk_fallback(fonts: &mut egui::FontDefinitions, family: egui::FontFamily) {
    fonts
        .families
        .entry(family)
        .or_default()
        .push("cjk".to_owned());
}

/// Font IDs used by the receive and send widgets.
pub fn receive_font_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from(RECEIVE_FONT_FAMILY)))
}

pub fn send_font_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(Arc::from(SEND_FONT_FAMILY)))
}

/// Apply configured point sizes without changing widget visuals.
pub fn apply_text_sizes(ctx: &egui::Context, settings: &FontSettings) {
    ctx.global_style_mut(|style| {
        style.text_styles = [
            (
                egui::TextStyle::Small,
                egui::FontId::proportional(settings.ui_size * 0.69),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::proportional(settings.ui_size),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::proportional(settings.ui_size),
            ),
            (
                egui::TextStyle::Heading,
                egui::FontId::proportional(settings.ui_size * 1.38),
            ),
            (
                egui::TextStyle::Monospace,
                receive_font_id(settings.receive_size),
            ),
        ]
        .into();
    });
}

/// Bind the named receive/send families to egui's built-in fonts in tests.
///
/// Headless layout tests do not need to scan multi-megabyte system fonts; they
/// only need the custom family names to exist so text layout cannot panic.
#[cfg(test)]
pub(crate) fn install_default_families(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let monospace = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    for name in [RECEIVE_FONT_FAMILY, SEND_FONT_FAMILY] {
        fonts
            .families
            .insert(egui::FontFamily::Name(Arc::from(name)), monospace.clone());
    }
    ctx.set_fonts(fonts);
}

/// Install the application font stack.
///
/// Zed default UI font is made primary for proportional text, Zed Mono for
/// monospace text, and the CJK face is appended as fallback for both.
pub fn install(ctx: &egui::Context, settings: &FontSettings) -> FontStatus {
    let mut fonts = egui::FontDefinitions::default();
    let builtin_monospace = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let cjk = find_cjk_font();
    let ui_font = settings
        .ui_family
        .as_deref()
        .and_then(find_family)
        .or_else(|| cjk.clone())
        .or_else(find_ui_font);
    if let Some(ui_font) = ui_font {
        add_font(&mut fonts, "ui", &ui_font);
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".to_owned());
        let _ = INSTALLED_UI_FAMILY.set(Some(ui_font.family));
    } else {
        let _ = INSTALLED_UI_FAMILY.set(None);
    }

    // If there is no CJK monospace face, using the CJK UI face is still
    // better than mixing a Latin monospace with a CJK fallback: the latter
    // reintroduces the one-pixel baseline split on Windows/macOS.
    let default_mono = find_mono_font().or_else(|| cjk.clone());
    let mono_keys = if default_mono.is_some() {
        vec!["mono".to_owned()]
    } else {
        builtin_monospace
    };
    if let Some(mono_font) = default_mono.clone() {
        add_font(&mut fonts, "mono", &mono_font);
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "mono".to_owned());
    }

    if let Some(ref font) = cjk {
        let mut data = egui::FontData::from_owned(font.data.clone());
        data.index = font.index;
        fonts.font_data.insert("cjk".to_owned(), Arc::new(data));

        push_cjk_fallback(&mut fonts, egui::FontFamily::Proportional);
        push_cjk_fallback(&mut fonts, egui::FontFamily::Monospace);
    }

    let receive_custom = settings.receive_family.as_deref().and_then(find_family);
    let send_custom = settings.send_family.as_deref().and_then(find_family);
    let receive_family = egui::FontFamily::Name(Arc::from(RECEIVE_FONT_FAMILY));
    let send_family = egui::FontFamily::Name(Arc::from(SEND_FONT_FAMILY));
    match (&receive_custom, &send_custom) {
        // One loaded face can back both named families, avoiding two copies
        // of a multi-megabyte CJK font when the user chooses the same family.
        (Some(receive), Some(send))
            if receive.family == send.family && receive.index == send.index =>
        {
            add_font(&mut fonts, "area", receive);
            bind_font_key(&mut fonts, &receive_family, "area");
            bind_font_key(&mut fonts, &send_family, "area");
        }
        (Some(font), None) | (None, Some(font)) => {
            add_font(&mut fonts, "area", font);
            if receive_custom.is_some() {
                bind_font_key(&mut fonts, &receive_family, "area");
                bind_font_keys(&mut fonts, &send_family, &mono_keys);
            } else {
                bind_font_keys(&mut fonts, &receive_family, &mono_keys);
                bind_font_key(&mut fonts, &send_family, "area");
            }
        }
        (Some(receive), Some(send)) => {
            add_font(&mut fonts, RECEIVE_FONT_FAMILY, receive);
            add_font(&mut fonts, SEND_FONT_FAMILY, send);
            bind_font_key(
                &mut fonts,
                &egui::FontFamily::Name(Arc::from(RECEIVE_FONT_FAMILY)),
                RECEIVE_FONT_FAMILY,
            );
            bind_font_key(
                &mut fonts,
                &egui::FontFamily::Name(Arc::from(SEND_FONT_FAMILY)),
                SEND_FONT_FAMILY,
            );
        }
        (None, None) => {
            bind_font_keys(&mut fonts, &receive_family, &mono_keys);
            bind_font_keys(&mut fonts, &send_family, &mono_keys);
        }
    }
    if cjk.is_some() {
        push_cjk_fallback(&mut fonts, receive_family);
        push_cjk_fallback(&mut fonts, send_family);
    }

    // The fallback list may already exist if a custom family was unavailable.
    // Keeping exactly one `cjk` entry makes the definition deterministic.
    for family in [
        egui::FontFamily::Proportional,
        egui::FontFamily::Monospace,
        egui::FontFamily::Name(Arc::from(RECEIVE_FONT_FAMILY)),
        egui::FontFamily::Name(Arc::from(SEND_FONT_FAMILY)),
    ] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.dedup();
        }
    }
    ctx.set_fonts(fonts);

    match cjk {
        Some(font) => {
            let _ = INSTALLED_FAMILY.set(Some(font.family.clone()));
            FontStatus::Loaded(font.family)
        }
        None => {
            let _ = INSTALLED_FAMILY.set(None);
            FontStatus::Missing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_families_are_non_empty_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for name in PREFERRED_FAMILIES {
            assert!(!name.is_empty());
            assert!(seen.insert(*name), "duplicate family {name}");
        }
    }

    #[test]
    fn custom_receive_and_send_families_are_installed() {
        let families = available_families();
        let Some(family) = ["Noto Sans", "DejaVu Sans", "Arial", "Helvetica"]
            .into_iter()
            .find(|candidate| families.iter().any(|family| family == candidate))
        else {
            // A minimal CI image may only have egui's built-in fonts.
            return;
        };
        let settings = FontSettings {
            receive_family: Some(family.to_owned()),
            send_family: Some(family.to_owned()),
            ..Default::default()
        };
        let ctx = egui::Context::default();

        install(&ctx, &settings);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| ());
        output.textures_delta.clear();

        let receive = receive_font_id(settings.receive_size);
        let send = send_font_id(settings.send_size);
        let has_mesh = |font: egui::FontId| {
            ctx.fonts_mut(|fonts| {
                let galley = fonts.layout_no_wrap("A".to_owned(), font, egui::Color32::WHITE);
                galley
                    .rows
                    .iter()
                    .flat_map(|row| row.glyphs.iter())
                    .any(|glyph| glyph.chr == 'A' && !glyph.uv_rect.is_nothing())
            })
        };
        assert!(
            has_mesh(receive),
            "receive font did not provide a glyph mesh"
        );
        assert!(has_mesh(send), "send font did not provide a glyph mesh");
    }

    #[test]
    fn file_stem_strips_directory_and_extension() {
        assert_eq!(
            file_stem("/usr/share/fonts/x/NotoSansCJK-Regular.ttc"),
            "NotoSansCJK-Regular"
        );
        assert_eq!(file_stem("C:\\Windows\\Fonts\\msyh.ttc"), "msyh");
    }

    #[test]
    fn zed_font_can_be_found() {
        if let Some(font) = find_ui_font() {
            assert!(!font.data.is_empty());
            assert!(!font.family.is_empty());
        }
    }

    #[test]
    fn detection_never_panics() {
        // The result depends on the host, so only assert it is well formed.
        if let Some(font) = find_cjk_font() {
            assert!(!font.data.is_empty());
            assert!(!font.family.is_empty());
        }
    }
}
