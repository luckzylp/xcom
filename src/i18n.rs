//! Internationalisation: Simplified Chinese and English.
//!
//! # Why Chinese is the key
//!
//! The translation key is the original Simplified-Chinese string.  That keeps
//! the change mechanical (each literal is wrapped, never rewritten), keeps the
//! diff reviewable, and needs no separate id space.  It is the classic gettext
//! arrangement, and the obvious cost — a missing translation degrades to the
//! Chinese text rather than to an empty label — is the right trade for a tool
//! of this size.
//!
//! # Why the language is global
//!
//! [`tr`] reads a process-wide setting instead of taking a receiver, so a call
//! site is `tr("打开串口")` rather than `app.t("打开串口")`.  That matters more
//! than it looks: the short form can be applied mechanically and consistently
//! across hundreds of call sites, including inside closures and free functions
//! that have no access to the application struct.  The UI is single-window and
//! single-threaded, so there is nothing to race with.
//!
//! A missing translation is a defect, so [`tests::every_key_has_a_translation`]
//! scans the source for `tr(…)` literals and fails if any is absent from the
//! table.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// A language the interface can be shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    /// Simplified Chinese.
    #[default]
    ZhCn,
    /// English.
    EnUs,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::ZhCn, Lang::EnUs];

    /// Name of the language, always written in that language so it is legible
    /// to someone who cannot read the current one.
    pub fn label(self) -> &'static str {
        match self {
            Lang::ZhCn => "简体中文",
            Lang::EnUs => "English",
        }
    }
}

/// The active language, as an index into [`Lang::ALL`].
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The language currently in use.
pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::EnUs,
        _ => Lang::ZhCn,
    }
}

/// Switch the active language.
pub fn set_lang(lang: Lang) {
    let index = match lang {
        Lang::ZhCn => 0,
        Lang::EnUs => 1,
    };
    CURRENT.store(index, Ordering::Relaxed);
}

/// Translate a source string.
///
/// Chinese is returned as-is; English is looked up, falling back to the source
/// string so a missing entry is visible rather than blank.
pub fn tr(zh: &'static str) -> &'static str {
    match lang() {
        Lang::ZhCn => zh,
        Lang::EnUs => table().get(zh).copied().unwrap_or(zh),
    }
}

/// Translate and substitute the first `{}`.
pub fn tr1(zh: &'static str, a: impl std::fmt::Display) -> String {
    tr(zh).replacen("{}", &a.to_string(), 1)
}

/// Translate and substitute two `{}` placeholders, left to right.
pub fn tr2(zh: &'static str, a: impl std::fmt::Display, b: impl std::fmt::Display) -> String {
    tr1(zh, a).replacen("{}", &b.to_string(), 1)
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| EN.iter().copied().collect())
}

/// Classify a locale string such as `zh_CN.UTF-8`.
///
/// Split out from [`system_lang`] so it can be tested without touching the
/// process environment.
pub fn classify(locale: Option<&str>) -> Lang {
    match locale {
        Some(value) if value.trim().to_ascii_lowercase().starts_with("zh") => Lang::ZhCn,
        _ => Lang::EnUs,
    }
}

/// Guess the language from the environment.
pub fn system_lang() -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return classify(Some(&value));
            }
        }
    }
    Lang::EnUs
}

/// English translations, keyed by the Simplified-Chinese source string.
///
/// Entries are grouped by the module they come from.  Order within the list is
/// irrelevant: [`table`] builds a map.
#[rustfmt::skip]
static EN: &[(&str, &str)] = &[
    // -- top bar ------------------------------------------------------------
    ("主题",                 "Theme"),
    ("语言",                 "Language"),
    ("显示工具面板",          "Show tool panel"),
    ("自定义接收区配色",       "Custom receive colours"),
    ("字色",                 "Text"),
    ("底色",                 "Background"),
    ("接收区字号",            "Receive font size"),
    ("关于",                 "About"),
    ("恢复默认设置",          "Restore defaults"),
    ("已恢复默认设置",         "Defaults restored"),
    ("关于 XCOM",            "About XCOM"),

    // -- theme names (src/theme.rs) ----------------------------------------
    ("黑色主题",              "Dark theme"),
    ("白色主题",              "Light theme"),
    ("跟随系统",              "Follow system"),

    // -- serial settings ----------------------------------------------------
    ("串口设置",              "Serial port"),
    ("串口",                  "Port"),
    ("（无可用串口）",         "(no ports found)"),
    ("未检测到串口",           "No serial ports detected"),
    ("刷新",                  "Refresh"),
    ("已刷新串口列表",         "Port list refreshed"),
    ("波特率",                "Baud rate"),
    ("（自定义）",            "(custom)"),
    ("自定义 ",               "Custom "),
    ("数据位",                "Data bits"),
    ("停止位",                "Stop bits"),
    ("校验位",                "Parity"),
    ("流控",                  "Flow control"),
    ("DTR / RTS",            "DTR / RTS"),
    ("打开串口",              "Open port"),
    ("关闭串口",              "Close port"),
    ("当前: {}",              "Current: {}"),
    ("提示: 串口打开时无法修改通信参数，需先关闭串口",
                              "Note: close the port before changing its parameters"),

    // -- parity / flow control (src/serial/config.rs) -----------------------
    ("无",                    "None"),
    ("奇校验",                "Odd"),
    ("偶校验",                "Even"),
    ("软件 (XON/XOFF)",       "Software (XON/XOFF)"),
    ("{} / {} 字节",          "{} / {} bytes"),
    ("硬件 (RTS/CTS)",        "Hardware (RTS/CTS)"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Serialises the tests that touch the process-wide language.
    static LANG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct Restore;

    impl Drop for Restore {
        fn drop(&mut self) {
            set_lang(Lang::ZhCn);
        }
    }

    #[test]
    fn table_has_no_duplicates_or_empty_entries() {
        let mut seen = std::collections::HashSet::new();
        for (key, value) in EN {
            assert!(!key.trim().is_empty(), "empty key");
            assert!(!value.trim().is_empty(), "empty translation for {key:?}");
            assert!(seen.insert(*key), "duplicate key {key:?}");
        }
        // The map must actually contain everything the list declares.
        assert_eq!(table().len(), EN.len());
    }

    #[test]
    fn every_translation_is_actually_in_english() {
        // A translation that is identical to the key is almost always a
        // forgotten entry rather than a deliberate loanword.
        for (key, value) in EN {
            if key.is_ascii() {
                continue; // e.g. "DTR / RTS" is the same in both languages.
            }
            assert_ne!(key, value, "{key:?} is untranslated");
        }
    }

    #[test]
    fn chinese_passes_through_untouched() {
        let _guard = LANG_LOCK.lock().unwrap();
        let _restore = Restore;
        set_lang(Lang::ZhCn);
        assert_eq!(tr("打开串口"), "打开串口");
        assert_eq!(tr1("当前: {}", 9600), "当前: 9600");
    }

    #[test]
    fn english_is_looked_up() {
        let _guard = LANG_LOCK.lock().unwrap();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr("打开串口"), "Open port");
        assert_eq!(tr("关闭串口"), "Close port");
        assert_ne!(tr("打开串口"), tr("关闭串口"));
    }

    #[test]
    fn unknown_keys_fall_back_to_the_source() {
        let _guard = LANG_LOCK.lock().unwrap();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr("这个键故意不存在"), "这个键故意不存在");
    }

    #[test]
    fn placeholders_are_substituted_left_to_right() {
        let _guard = LANG_LOCK.lock().unwrap();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr1("当前: {}", "115200"), "Current: 115200");
        assert_eq!(tr2("{} / {}", 1, 4), "1 / 4");
        // A key without a placeholder ignores the argument.
        assert_eq!(tr1("刷新", 1), "Refresh");
    }

    #[test]
    fn locale_classification() {
        assert_eq!(classify(Some("zh_CN.UTF-8")), Lang::ZhCn);
        assert_eq!(classify(Some("zh")), Lang::ZhCn);
        assert_eq!(classify(Some("ZH_TW")), Lang::ZhCn);
        assert_eq!(classify(Some("en_US.UTF-8")), Lang::EnUs);
        assert_eq!(classify(Some("de_DE")), Lang::EnUs);
        assert_eq!(classify(Some("")), Lang::EnUs);
        assert_eq!(classify(None), Lang::EnUs);
    }

    #[test]
    fn language_labels_are_in_their_own_language() {
        assert_eq!(Lang::ZhCn.label(), "简体中文");
        assert_eq!(Lang::EnUs.label(), "English");
    }

    /// Collect every `src/**/*.rs` path, excluding this module.
    fn source_files() -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().is_some_and(|n| n != "i18n.rs")
                {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
            &mut out,
        );
        out.sort();
        out
    }

    /// Extract the first string literal of every `tr(`/`tr1(`/`tr2(`/`tr3(` call.
    fn literals_in(source: &str) -> Vec<String> {
        let bytes = source.as_bytes();
        let mut found = Vec::new();
        for name in ["tr1(", "tr2(", "tr3(", "tr("] {
            let mut from = 0usize;
            while let Some(offset) = source[from..].find(name) {
                let start = from + offset;
                // Require a word boundary so `attr(` does not match `tr(`.
                let preceded_by_word = start > 0
                    && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
                from = start + name.len();
                if preceded_by_word {
                    continue;
                }
                let rest = &source[from..];
                let trimmed = rest.trim_start();
                let skipped = rest.len() - trimmed.len();
                if !trimmed.starts_with('"') {
                    continue; // a variable or a formatted argument, not a literal
                }
                // Decode the literal, honouring backslash escapes.
                let mut value = String::new();
                let mut chars = trimmed[1..].chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => value.push('\n'),
                            Some('t') => value.push('\t'),
                            Some('\\') => value.push('\\'),
                            Some('"') => value.push('"'),
                            Some(other) => value.push(other),
                            None => break,
                        },
                        other => value.push(other),
                    }
                }
                let _ = skipped;
                found.push(value);
            }
        }
        found
    }

    #[test]
    fn every_key_has_a_translation() {
        let known = table();
        let mut missing = Vec::new();
        let mut checked = 0usize;
        for path in source_files() {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for literal in literals_in(&source) {
                // Only CJK strings are translation keys; ASCII ones (format
                // fragments, units) are passed through deliberately.
                if !literal
                    .chars()
                    .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
                {
                    continue;
                }
                checked += 1;
                if !known.contains_key(literal.as_str()) {
                    missing.push(format!("{}: {literal:?}", path.display()));
                }
            }
        }
        assert!(checked > 0, "the scanner found no call sites at all");
        assert!(
            missing.is_empty(),
            "missing translations:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn the_scanner_actually_finds_call_sites() {
        // Guards the guard: a broken scanner would make the test above vacuous.
        let sample = r#"ui.label(tr("刷新")); let a = tr1("当前: {}", x);"#;
        let mut found = literals_in(sample);
        found.sort();
        assert_eq!(found, vec!["刷新", "当前: {}"]);
        // Word boundaries: `attr(` must not be mistaken for `tr(`.
        assert!(literals_in(r#"foo.attr("x")"#).is_empty());
    }
}
