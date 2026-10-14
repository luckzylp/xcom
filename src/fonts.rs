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
pub struct LoadedFont {
    /// Family name, for display in the UI.
    pub family: String,
    pub data: Vec<u8>,
    /// Face index within a font collection (`.ttc`).
    pub index: u32,
}

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
    "ZedMono Nerd Font Mono",
    "ZedMono NFM",
    "ZedMono Nerd Font",
    "IBM Plex Mono",
    "JetBrains Mono",
    "DejaVu Sans Mono",
];

/// Try to find a Zed-like UI font.
pub fn find_ui_font() -> Option<LoadedFont> {
    for path in KNOWN_UI_PATHS {
        if let Ok(data) = std::fs::read(path) {
            if !data.is_empty() {
                return Some(LoadedFont {
                    family: file_stem(path),
                    data,
                    index: 0,
                });
            }
        }
    }
    scan_families(PREFERRED_UI_FAMILIES)
}

/// Try to find a Zed-like Monospace font.
pub fn find_mono_font() -> Option<LoadedFont> {
    for path in KNOWN_MONO_PATHS {
        if let Ok(data) = std::fs::read(path) {
            if !data.is_empty() {
                return Some(LoadedFont {
                    family: file_stem(path),
                    data,
                    index: 0,
                });
            }
        }
    }
    scan_families(PREFERRED_MONO_FAMILIES)
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
        if let Ok(data) = std::fs::read(path) {
            if !data.is_empty() {
                return Some(LoadedFont {
                    family: file_stem(path),
                    data,
                    index: 0,
                });
            }
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
            .map(|face| {
                (
                    face.id,
                    face.index,
                    face.families.first().map(|(n, _)| n.clone()),
                )
            });
        let Some((id, index, name)) = family else {
            continue;
        };
        let data = db.with_face_data(id, |bytes, _| bytes.to_vec())?;
        if data.is_empty() {
            continue;
        }
        return Some(LoadedFont {
            family: name.unwrap_or_else(|| (*wanted).to_owned()),
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

/// Install the application font stack.
///
/// Zed default UI font is made primary for proportional text, Zed Mono for
/// monospace text, and the CJK face is appended as fallback for both.
pub fn install(ctx: &egui::Context) -> FontStatus {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(ui_font) = find_ui_font() {
        let mut data = egui::FontData::from_owned(ui_font.data);
        data.index = ui_font.index;
        fonts.font_data.insert("zed_ui".to_owned(), Arc::new(data));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "zed_ui".to_owned());
        let _ = INSTALLED_UI_FAMILY.set(Some(ui_font.family));
    } else {
        let _ = INSTALLED_UI_FAMILY.set(None);
    }

    if let Some(mono_font) = find_mono_font() {
        let mut data = egui::FontData::from_owned(mono_font.data);
        data.index = mono_font.index;
        fonts.font_data.insert("zed_mono".to_owned(), Arc::new(data));
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "zed_mono".to_owned());
    }

    let cjk = find_cjk_font();
    if let Some(ref font) = cjk {
        let mut data = egui::FontData::from_owned(font.data.clone());
        data.index = font.index;
        fonts.font_data.insert("cjk".to_owned(), Arc::new(data));

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_owned());
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
