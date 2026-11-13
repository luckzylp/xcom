//! Interface, receive-area and send-area font selection.
//!
//! GPUI renders with the platform's text system, so there is no font-definition
//! table to install.  This module scans the installed families for the font
//! dialog, chooses a CJK-capable family for the one-time warning, and turns a
//! [`FontSettings`] into the `Font` values the views ask for.

use std::sync::OnceLock;

use gpui_kit::{Font, SharedString, font};

use crate::settings::FontSettings;

/// Families reported by `fontdb`, sorted and de-duplicated.
///
/// Scanning the system fonts is a few hundred milliseconds on a cold cache, so
/// the result is kept for the life of the process.
fn installed_families() -> &'static [(String, bool)] {
    static FAMILIES: OnceLock<Vec<(String, bool)>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut families: Vec<(String, bool)> = db
            .faces()
            .filter_map(|face| {
                face.families
                    .first()
                    .map(|(name, _)| (name.clone(), face.monospaced))
            })
            .collect();
        families.sort_by_key(|a| a.0.to_lowercase());
        families.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
        families
    })
}

/// Every installed family, for the font settings dialog.
pub fn available_families() -> Vec<String> {
    installed_families()
        .iter()
        .map(|(name, _)| name.clone())
        .collect()
}

/// A sensible default interface family when the user has not chosen one.
///
/// GPUI's `.SystemUIFont` sometimes resolves to a family that is not actually
/// installed on Linux; naming an installed family explicitly avoids the first
/// frame's failed font lookup and keeps CJK glyphs available.
pub fn default_ui_family() -> Option<String> {
    if let Some(cjk) = find_cjk_family() {
        return Some(cjk);
    }
    const PREFERRED: &[&str] = &[
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
        "FreeSans",
        "Arial",
        "Helvetica",
    ];
    let installed = installed_families();
    for preferred in PREFERRED {
        if installed
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(preferred))
        {
            return Some((*preferred).to_owned());
        }
    }
    installed.first().map(|(name, _)| name.clone())
}

/// The first installed family that can render CJK text, if any.
///
/// This is advisory: GPUI falls back through the platform's font list, so a
/// missing CJK family means a warning rather than a broken interface.
pub fn find_cjk_family() -> Option<String> {
    const PREFERRED: &[&str] = &[
        "Noto Sans CJK SC",
        "Noto Sans CJK",
        "Source Han Sans SC",
        "Source Han Sans",
        "Microsoft YaHei",
        "微软雅黑",
        "PingFang SC",
        "PingFang",
        "WenQuanYi Micro Hei",
        "WenQuanYi Zen Hei",
        "SimSun",
        "宋体",
        "Droid Sans Fallback",
    ];
    let installed = installed_families();
    for preferred in PREFERRED {
        if installed
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(preferred))
        {
            return Some((*preferred).to_owned());
        }
    }
    // A family whose name advertises CJK coverage, even if it is not in the
    // short preferred list (localised distributions rename their packs).
    installed
        .iter()
        .find(|(name, _)| {
            let lower = name.to_lowercase();
            lower.contains("cjk")
                || lower.contains("han sans")
                || lower.contains("noto sans sc")
                || lower.contains("wenquanyi")
        })
        .map(|(name, _)| name.clone())
}

fn family_or(family: Option<&str>, fallback: &SharedString) -> SharedString {
    family
        .map(SharedString::from)
        .unwrap_or_else(|| fallback.clone())
}

/// The receive-area font, falling back to the theme's installed monospace face.
pub fn receive_font(settings: &FontSettings, mono: &SharedString) -> Font {
    font(family_or(settings.receive_family.as_deref(), mono))
}

/// The send-area font, falling back to the theme's installed monospace face.
pub fn send_font(settings: &FontSettings, mono: &SharedString) -> Font {
    font(family_or(settings.send_family.as_deref(), mono))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_sorted_and_unique() {
        let families = available_families();
        let lower: Vec<String> = families.iter().map(|f| f.to_lowercase()).collect();
        let mut sorted = lower.clone();
        sorted.sort();
        assert_eq!(lower, sorted, "the font list must be sorted");
        let unique: std::collections::HashSet<_> = lower.iter().collect();
        assert_eq!(unique.len(), lower.len(), "duplicate families");
    }

    #[test]
    fn cjk_detection_accepts_common_names() {
        // The function is advisory and scans the live system, so only assert
        // the shape of a hit: a name is returned and is non-empty.
        if let Some(family) = find_cjk_family() {
            assert!(!family.trim().is_empty());
        }
    }

    #[test]
    fn family_selection_uses_the_configured_name() {
        let settings = FontSettings {
            ui_family: Some("Test UI".to_owned()),
            receive_family: Some("Test Mono".to_owned()),
            send_family: Some("Test Send".to_owned()),
            ..Default::default()
        };
        let mono = SharedString::from("Mono Fallback");
        assert_eq!(receive_font(&settings, &mono).family.as_ref(), "Test Mono");
        assert_eq!(send_font(&settings, &mono).family.as_ref(), "Test Send");
    }

    #[test]
    fn missing_selection_falls_back_to_the_mono_face() {
        let settings = FontSettings::default();
        let mono = SharedString::from("Mono Fallback");
        assert_eq!(
            receive_font(&settings, &mono).family.as_ref(),
            "Mono Fallback"
        );
        assert_eq!(send_font(&settings, &mono).family.as_ref(), "Mono Fallback");
    }
}
