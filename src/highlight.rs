//! Keyword highlighting for the receive view.
//!
//! Rules are literal, case-sensitive substrings.  Matching is non-overlapping
//! and scans left to right: at each position the longest rule wins, so a rule
//! for `AT+RST` is not cut short by a rule for `AT`.  The resulting ranges are
//! what the log view turns into coloured `LayoutJob` sections; the row text
//! itself is never changed, which keeps caret columns and copied text exact.

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
    /// Literal text to look for.  Empty rules are ignored but kept in the
    /// editor so a half-typed rule is not silently deleted.
    pub text: String,
    /// RGB colour used for the match.
    pub color: [u8; 3],
}

impl Default for HighlightRule {
    fn default() -> Self {
        Self {
            text: String::new(),
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

    /// Whether there is anything to paint.
    pub fn is_active(&self) -> bool {
        self.enabled && self.rules.iter().any(|rule| !rule.text.is_empty())
    }

    /// Non-overlapping matches in `text`, ordered by position.
    ///
    /// Empty rules are skipped.  At the same start offset the longest rule
    /// wins; if two rules tie, the earlier one in the list is used.
    pub fn matches(&self, text: &str) -> Vec<HighlightMatch> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < text.len() {
            let mut best: Option<HighlightMatch> = None;
            for (index, rule) in self.rules.iter().enumerate() {
                if rule.text.is_empty() || !text[pos..].starts_with(&rule.text) {
                    continue;
                }
                let end = pos + rule.text.len();
                let candidate = HighlightMatch {
                    start: pos,
                    end,
                    rule: index,
                };
                if best.is_none_or(|current| candidate.end > current.end) {
                    best = Some(candidate);
                }
            }
            match best {
                Some(found) => {
                    pos = found.end;
                    out.push(found);
                }
                None => {
                    pos += text[pos..].chars().next().map(char::len_utf8).unwrap_or(1);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(text: &str, color: [u8; 3]) -> HighlightRule {
        HighlightRule {
            text: text.to_owned(),
            color,
        }
    }

    #[test]
    fn finds_every_literal_occurrence() {
        let settings = HighlightSettings {
            enabled: true,
            rules: vec![rule("OK", [1, 2, 3])],
        };
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
        let settings = HighlightSettings {
            enabled: true,
            rules: vec![rule("AT", [1, 1, 1]), rule("AT+RST", [2, 2, 2])],
        };
        let matches = settings.matches("AT+RST");
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].start, matches[0].end), (0, 6));
        assert_eq!(matches[0].rule, 1, "the longer rule must be chosen");
    }

    #[test]
    fn matching_is_case_sensitive() {
        let settings = HighlightSettings {
            enabled: true,
            rules: vec![rule("OK", [1, 2, 3])],
        };
        assert!(settings.matches("ok").is_empty());
    }

    #[test]
    fn unicode_rules_match_on_char_boundaries() {
        let settings = HighlightSettings {
            enabled: true,
            rules: vec![rule("温度", [1, 2, 3])],
        };
        let text = "当前温度: 25";
        let found = settings.matches(text);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].start..found[0].end], "温度");
    }

    #[test]
    fn empty_rules_and_disabled_settings_do_nothing() {
        let settings = HighlightSettings {
            enabled: false,
            rules: vec![rule("OK", [1, 2, 3])],
        };
        assert!(!settings.is_active());

        let empty = HighlightSettings {
            enabled: true,
            rules: vec![rule("", [1, 2, 3])],
        };
        assert!(!empty.is_active());
        assert!(empty.matches("anything").is_empty());
    }

    #[test]
    fn normalize_caps_the_rule_count() {
        let mut settings = HighlightSettings {
            enabled: true,
            rules: (0..MAX_RULES + 10)
                .map(|i| rule(&format!("k{i}"), [1, 2, 3]))
                .collect(),
        };
        settings.normalize();
        assert_eq!(settings.rules.len(), MAX_RULES);
    }
}
