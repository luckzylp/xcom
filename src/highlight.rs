//! Keyword highlighting for the receive view.
//!
//! Each rule is either a literal, case-sensitive substring or a regular
//! expression.  Matching is non-overlapping and scans left to right: at each
//! position the longest match wins, so a rule for `AT+RST` is not cut short by
//! a rule for `AT`.  The resulting ranges are what the log view turns into
//! coloured `LayoutJob` sections; the row text itself is never changed, which
//! keeps caret columns and copied text exact.
//!
//! Compiling a regular expression is the expensive part, so the view keeps a
//! [`Highlighter`] and only rebuilds it when the settings change.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Upper bound on configured rules, so a hand-edited settings file cannot turn
/// every rendered row into an unbounded matching loop.
pub const MAX_RULES: usize = 32;

/// The colour new rules start with: a warm gold that reads on both themes.
pub const DEFAULT_COLOR: [u8; 3] = [0xFF, 0xD1, 0x66];

/// One keyword and the colour it is painted in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlightRule {
    /// Text or pattern to look for.  Empty rules are ignored but kept in the
    /// editor so a half-typed rule is not silently deleted.
    pub text: String,
    /// Treat `text` as a regular expression instead of a literal substring.
    pub regex: bool,
    /// RGB colour used for the match.
    pub color: [u8; 3],
}

impl Default for HighlightRule {
    fn default() -> Self {
        Self {
            text: String::new(),
            regex: false,
            color: DEFAULT_COLOR,
        }
    }
}

/// The complete keyword-highlighting configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlightSettings {
    pub enabled: bool,
    pub rules: Vec<HighlightRule>,
}

/// One matched range in the row text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightMatch {
    /// Byte offset where the match starts.
    pub start: usize,
    /// Byte offset just past the match.
    pub end: usize,
    /// Index into [`HighlightSettings::rules`].
    pub rule: usize,
}

impl HighlightSettings {
    /// Clamp anything a hand-edited settings file could exaggerate.
    pub fn normalize(&mut self) {
        self.rules.truncate(MAX_RULES);
    }
}

/// Whether `pattern` is a regular expression the engine accepts.
///
/// The settings editor uses this to flag a half-typed pattern; the highlighter
/// itself silently skips invalid rules so a hand-edited file cannot break the
/// view.
pub fn validate_regex(pattern: &str) -> Result<(), String> {
    Regex::new(pattern)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// A rule prepared for matching: literal rules avoid the regex engine entirely.
#[derive(Debug)]
struct CompiledRule {
    text: String,
    regex: Option<Regex>,
    /// Index into the original rule list, so the painter gets the colour.
    rule: usize,
    color: [u8; 3],
}

/// The compiled form of [`HighlightSettings`], cached by the log view.
#[derive(Debug, Default)]
pub struct Highlighter {
    rules: Vec<CompiledRule>,
    /// Settings the cache was built from, compared to detect edits.
    settings: HighlightSettings,
    active: bool,
}

impl Highlighter {
    /// Compile every usable rule in `settings`.
    ///
    /// Invalid regular expressions are skipped: the editor marks them, but a
    /// hand-edited settings file must never stop the receive view from drawing.
    pub fn new(settings: &HighlightSettings) -> Self {
        let mut rules = Vec::new();
        if settings.enabled {
            for (index, rule) in settings.rules.iter().enumerate() {
                if rule.text.is_empty() {
                    continue;
                }
                let regex = if rule.regex {
                    match Regex::new(&rule.text) {
                        Ok(regex) => Some(regex),
                        Err(err) => {
                            log::warn!("关键字正则无效（{}）：{err}", rule.text);
                            continue;
                        }
                    }
                } else {
                    None
                };
                rules.push(CompiledRule {
                    text: rule.text.clone(),
                    regex,
                    rule: index,
                    color: rule.color,
                });
            }
        }
        Self {
            active: settings.enabled && !rules.is_empty(),
            rules,
            settings: settings.clone(),
        }
    }

    /// Rebuild the cache only when the settings really changed.
    pub fn sync(&mut self, settings: &HighlightSettings) {
        if self.settings != *settings {
            *self = Self::new(settings);
        }
    }

    /// Whether there is anything to paint.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Colour configured for the rule at `rule` (an index into the settings).
    pub fn color(&self, rule: usize) -> [u8; 3] {
        self.rules
            .iter()
            .find(|compiled| compiled.rule == rule)
            .map_or(DEFAULT_COLOR, |compiled| compiled.color)
    }

    /// Non-overlapping matches in `text`, ordered by position.
    ///
    /// Empty rules are skipped.  At the same start offset the longest match
    /// wins; if two rules tie, the earlier one in the list is used.
    pub fn matches(&self, text: &str) -> Vec<HighlightMatch> {
        if !self.active || text.is_empty() {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        for rule in &self.rules {
            match &rule.regex {
                Some(regex) => {
                    for found in regex.find_iter(text) {
                        // Zero-width matches would paint nothing and are the
                        // only way a pattern could make the scan loop.
                        if found.end() > found.start() {
                            candidates.push(HighlightMatch {
                                start: found.start(),
                                end: found.end(),
                                rule: rule.rule,
                            });
                        }
                    }
                }
                None => {
                    for (start, _) in text.match_indices(rule.text.as_str()) {
                        candidates.push(HighlightMatch {
                            start,
                            end: start + rule.text.len(),
                            rule: rule.rule,
                        });
                    }
                }
            }
        }

        // Leftmost first; at one start position the longest wins, and on a tie
        // the earlier rule does.  Overlapping candidates are then dropped.
        candidates.sort_by_key(|found| (found.start, std::cmp::Reverse(found.end), found.rule));
        let mut out = Vec::new();
        let mut cursor = 0;
        for found in candidates {
            if found.start < cursor {
                continue;
            }
            cursor = found.end;
            out.push(found);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(text: &str, color: [u8; 3]) -> HighlightRule {
        HighlightRule {
            text: text.to_owned(),
            regex: false,
            color,
        }
    }

    fn pattern(text: &str, color: [u8; 3]) -> HighlightRule {
        HighlightRule {
            text: text.to_owned(),
            regex: true,
            color,
        }
    }

    fn highlighter(rules: Vec<HighlightRule>) -> Highlighter {
        Highlighter::new(&HighlightSettings {
            enabled: true,
            rules,
        })
    }

    #[test]
    fn finds_every_literal_occurrence() {
        let settings = highlighter(vec![literal("OK", [1, 2, 3])]);
        let matches = settings.matches("OK then OK");
        assert_eq!(
            matches,
            vec![
                HighlightMatch {
                    start: 0,
                    end: 2,
                    rule: 0
                },
                HighlightMatch {
                    start: 8,
                    end: 10,
                    rule: 0
                },
            ]
        );
    }

    #[test]
    fn the_longest_rule_wins_at_a_position() {
        let settings = highlighter(vec![literal("AT", [1, 1, 1]), literal("AT+RST", [2, 2, 2])]);
        let matches = settings.matches("AT+RST");
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].start, matches[0].end), (0, 6));
        assert_eq!(matches[0].rule, 1, "the longer rule must be chosen");
    }

    #[test]
    fn matching_is_case_sensitive() {
        let settings = highlighter(vec![literal("OK", [1, 2, 3])]);
        assert!(settings.matches("ok").is_empty());
    }

    #[test]
    fn unicode_rules_match_on_char_boundaries() {
        let settings = highlighter(vec![literal("温度", [1, 2, 3])]);
        let text = "当前温度: 25";
        let found = settings.matches(text);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].start..found[0].end], "温度");
    }

    #[test]
    fn empty_rules_and_disabled_settings_do_nothing() {
        let disabled = HighlightSettings {
            enabled: false,
            rules: vec![literal("OK", [1, 2, 3])],
        };
        assert!(!Highlighter::new(&disabled).is_active());
        assert!(Highlighter::new(&disabled).matches("OK").is_empty());

        let empty = HighlightSettings {
            enabled: true,
            rules: vec![literal("", [1, 2, 3])],
        };
        assert!(!Highlighter::new(&empty).is_active());
        assert!(Highlighter::new(&empty).matches("anything").is_empty());
    }

    #[test]
    fn normalize_caps_the_rule_count() {
        let mut settings = HighlightSettings {
            enabled: true,
            rules: (0..MAX_RULES + 10)
                .map(|i| literal(&format!("k{i}"), [1, 2, 3]))
                .collect(),
        };
        settings.normalize();
        assert_eq!(settings.rules.len(), MAX_RULES);
    }

    #[test]
    fn regex_rules_find_every_non_overlapping_match() {
        let settings = highlighter(vec![pattern("ERR\\d+", [1, 2, 3])]);
        let matches = settings.matches("ERR1 ok ERR22 ERR333");
        let found: Vec<&str> = matches
            .iter()
            .map(|m| &"ERR1 ok ERR22 ERR333"[m.start..m.end])
            .collect();
        assert_eq!(found, vec!["ERR1", "ERR22", "ERR333"]);
    }

    #[test]
    fn regex_anchors_apply_to_the_whole_row() {
        let settings = highlighter(vec![pattern("^ERROR", [1, 2, 3])]);
        assert_eq!(settings.matches("ERROR: bad").len(), 1);
        assert!(settings.matches("an ERROR").is_empty());
    }

    #[test]
    fn a_longer_regex_beats_a_literal_at_the_same_start() {
        let settings = highlighter(vec![
            literal("AT", [1, 1, 1]),
            pattern("AT\\+RST", [2, 2, 2]),
        ]);
        let matches = settings.matches("AT+RST");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            (matches[0].start, matches[0].end, matches[0].rule),
            (0, 6, 1)
        );
    }

    #[test]
    fn zero_width_regex_matches_paint_nothing() {
        let settings = highlighter(vec![pattern("^", [1, 2, 3])]);
        assert!(settings.matches("anything").is_empty());
    }

    #[test]
    fn an_invalid_regex_is_skipped_but_other_rules_still_work() {
        let settings = Highlighter::new(&HighlightSettings {
            enabled: true,
            rules: vec![pattern("(unclosed", [1, 2, 3]), literal("OK", [4, 5, 6])],
        });
        assert!(settings.is_active());
        let matches = settings.matches("OK");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule, 1);
        assert!(validate_regex("(unclosed").is_err());
        assert!(validate_regex("ERR\\d+").is_ok());
    }

    #[test]
    fn sync_rebuilds_only_when_the_rules_change() {
        let mut highlighter = Highlighter::new(&HighlightSettings::default());
        assert!(!highlighter.is_active());

        let settings = HighlightSettings {
            enabled: true,
            rules: vec![pattern("\\d+", [1, 2, 3])],
        };
        highlighter.sync(&settings);
        assert!(highlighter.is_active());
        assert_eq!(highlighter.matches("a42").len(), 1);

        let unchanged = settings.clone();
        highlighter.sync(&unchanged);
        assert_eq!(highlighter.matches("a42").len(), 1);
    }
}
