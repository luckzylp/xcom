//! The multi-send panel: pages of preset commands with optional notes.
//!
//! The original tool offers four pages of ten entries, each optionally sent
//! with a newline and/or as hex, with the ability to loop through the checked
//! entries on a timer and to bind the first ten to the numeric keypad.

use serde::{Deserialize, Serialize};

/// Number of pages.
pub const PAGES: usize = 4;
/// Entries per page.
/// Entries allocated per page up front.  A page grows beyond this as the
/// panel gets taller; see [`MultiSend::ensure_len`].
pub const ENTRIES_PER_PAGE: usize = 10;

/// Upper bound on how many entries a page may grow to, so a tall window
/// cannot make the settings file unbounded.
pub const MAX_ENTRIES_PER_PAGE: usize = 200;

/// One preset command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiEntry {
    /// The command text (hex form allowed when `hex` is set).
    pub text: String,
    /// Interpret `text` as hexadecimal bytes.
    pub hex: bool,
    /// Append CR/LF after the payload.
    pub newline: bool,
    /// Free-form description shown next to the entry.
    pub note: String,
    /// Delay applied *after* this entry is sent, before the next one, in
    /// milliseconds.  Lets a sequence give a device time to answer.
    pub delay_ms: u64,
    /// Include this entry when looping.
    pub enabled: bool,
}

impl MultiEntry {
    /// Whether this entry has anything to send.
    pub fn is_sendable(&self) -> bool {
        !self.text.trim().is_empty()
    }
}

/// The whole multi-send panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiSend {
    /// `pages[page][index]`.
    pub pages: Vec<Vec<MultiEntry>>,
    /// Currently displayed page.
    pub page: usize,
    /// Loop through all enabled entries on a timer.
    ///
    /// This is a *run* state, not a preference, so it is deliberately not
    /// persisted: a ticked box that outlives a restart — or a port close —
    /// reads as "still looping" while nothing is running.
    #[serde(skip)]
    pub loop_enabled: bool,
    pub loop_period_ms: u64,
    /// How many full passes the loop makes.  `None` means loop forever.
    pub loop_count: Option<u32>,
    /// Bind entries 0-9 to the numeric keypad.
    pub keypad_enabled: bool,
    /// Append CR/LF to entries that request it (CRLF by default).
    pub newline_cr: bool,
    pub newline_lf: bool,
}

impl Default for MultiSend {
    fn default() -> Self {
        Self {
            pages: vec![vec![MultiEntry::default(); ENTRIES_PER_PAGE]; PAGES],
            page: 0,
            loop_enabled: false,
            loop_period_ms: 1000,
            loop_count: None,
            keypad_enabled: false,
            newline_cr: true,
            newline_lf: true,
        }
    }
}

impl MultiSend {
    /// Ensure the page table has the expected shape after loading old settings.
    pub fn normalize(&mut self) {
        self.pages.resize(PAGES, Vec::new());
        for page in self.pages.iter_mut() {
            page.resize(ENTRIES_PER_PAGE, MultiEntry::default());
        }
        self.page = self.page.min(PAGES - 1);
        self.loop_period_ms = self.loop_period_ms.clamp(1, 3_600_000);
        for page in self.pages.iter_mut() {
            for entry in page.iter_mut() {
                entry.delay_ms = entry.delay_ms.min(3_600_000);
            }
        }
        // Zero passes would look like a loop that silently does nothing.
        self.loop_count = self.loop_count.map(|n| n.clamp(1, 1_000_000));
    }

    /// Borrow one entry.
    pub fn entry(&self, page: usize, index: usize) -> Option<&MultiEntry> {
        self.pages.get(page)?.get(index)
    }

    /// Mutably borrow one entry.
    pub fn entry_mut(&mut self, page: usize, index: usize) -> Option<&mut MultiEntry> {
        self.pages.get_mut(page)?.get_mut(index)
    }

    /// Grow a page so that at least `len` entries exist.
    ///
    /// The panel shows as many rows as fit its height, so pages are sized by
    /// the window rather than being fixed at ten.
    /// Returns whether the page actually grew, so callers only persist a real
    /// change.
    pub fn ensure_len(&mut self, page: usize, len: usize) -> bool {
        let len = len.clamp(ENTRIES_PER_PAGE, MAX_ENTRIES_PER_PAGE);
        if let Some(entries) = self.pages.get_mut(page)
            && entries.len() < len
        {
            entries.resize(len, MultiEntry::default());
            return true;
        }
        false
    }

    /// The delay to apply after sending `index` on `page`.
    pub fn delay_after(&self, page: usize, index: usize) -> u64 {
        self.entry(page, index).map_or(0, |entry| entry.delay_ms)
    }

    pub fn first_page(&mut self) {
        self.page = 0;
    }

    pub fn previous_page(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    pub fn next_page(&mut self) {
        if self.page + 1 < self.pages.len() {
            self.page += 1;
        }
    }

    pub fn last_page(&mut self) {
        self.page = self.pages.len().saturating_sub(1);
    }

    /// The newline suffix for multi-send entries, in byte form.
    pub fn newline_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2);
        if self.newline_cr {
            out.push(b'\r');
        }
        if self.newline_lf {
            out.push(b'\n');
        }
        out
    }

    /// Encode one entry for transmission.
    ///
    /// Returns `Err` with a user-facing message when a hex entry is malformed,
    /// which is the one case the original tool also refuses to send.
    pub fn encode_entry(&self, entry: &MultiEntry) -> Result<Vec<u8>, String> {
        let mut bytes = if entry.hex {
            crate::hex::parse(&entry.text).map_err(|e| e.to_string())?
        } else {
            entry.text.as_bytes().to_vec()
        };
        if entry.newline {
            bytes.extend_from_slice(&self.newline_bytes());
        }
        Ok(bytes)
    }

    /// Indices of the checked, non-empty entries on a page, in order.
    ///
    /// Drives both the loop and the one-shot "send checked" action.
    pub fn enabled_indices(&self, page: usize) -> Vec<usize> {
        self.pages
            .get(page)
            .map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.enabled && e.is_sendable())
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Serialise all pages to a portable JSON document.
    pub fn export_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Doc<'a> {
            version: u32,
            pages: &'a Vec<Vec<MultiEntry>>,
        }
        serde_json::to_string_pretty(&Doc {
            version: 1,
            pages: &self.pages,
        })
        .map_err(|e| e.to_string())
    }

    /// Load pages from a JSON document produced by [`export_json`](Self::export_json).
    pub fn import_json(&mut self, text: &str) -> Result<usize, String> {
        #[derive(Deserialize)]
        struct Doc {
            #[serde(default)]
            pages: Vec<Vec<MultiEntry>>,
        }
        let doc: Doc =
            serde_json::from_str(text).map_err(|e| crate::i18n::tr1("解析失败: {}", e))?;
        if doc.pages.is_empty() {
            return Err(crate::i18n::tr("文件中没有条目").to_owned());
        }
        let count = doc.pages.iter().map(Vec::len).sum();
        self.pages = doc.pages;
        self.normalize();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shape_is_four_pages_of_ten() {
        let m = MultiSend::default();
        assert_eq!(m.pages.len(), PAGES);
        for page in &m.pages {
            assert_eq!(page.len(), ENTRIES_PER_PAGE);
        }
    }

    #[test]
    fn paging_clamps_at_both_ends() {
        let mut m = MultiSend::default();
        m.previous_page();
        assert_eq!(m.page, 0);
        m.first_page();
        assert_eq!(m.page, 0);
        for _ in 0..10 {
            m.next_page();
        }
        assert_eq!(m.page, PAGES - 1);
        m.last_page();
        assert_eq!(m.page, PAGES - 1);
    }

    #[test]
    fn encodes_text_entry_with_newline() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "AT".to_owned(),
            newline: true,
            ..Default::default()
        };
        assert_eq!(m.encode_entry(&entry).unwrap(), b"AT\r\n");
    }

    #[test]
    fn encodes_hex_entry() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "01 02 0A".to_owned(),
            hex: true,
            newline: false,
            ..Default::default()
        };
        assert_eq!(m.encode_entry(&entry).unwrap(), vec![0x01, 0x02, 0x0A]);
    }

    #[test]
    fn rejects_malformed_hex_entry() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "0G".to_owned(),
            hex: true,
            ..Default::default()
        };
        assert!(m.encode_entry(&entry).is_err());
    }

    #[test]
    fn enabled_indices_skip_disabled_and_empty_entries() {
        let mut m = MultiSend::default();
        for (index, (text, enabled)) in [("a", true), ("b", false), ("", true), ("c", true)]
            .into_iter()
            .enumerate()
        {
            let entry = m.entry_mut(0, index).expect("entry exists");
            entry.text = text.to_owned();
            entry.enabled = enabled;
        }
        assert_eq!(m.enabled_indices(0), vec![0, 3]);
    }

    #[test]
    fn json_round_trip_preserves_entries() {
        let mut m = MultiSend::default();
        *m.entry_mut(0, 3).expect("entry exists") = MultiEntry {
            text: "0A0B".to_owned(),
            hex: true,
            newline: true,
            note: "备注".to_owned(),
            delay_ms: 0,
            enabled: true,
        };
        let json = m.export_json().unwrap();
        let mut loaded = MultiSend::default();
        let count = loaded.import_json(&json).unwrap();
        assert_eq!(count, PAGES * ENTRIES_PER_PAGE);
        assert_eq!(loaded.pages[0][3], m.pages[0][3]);
    }

    /// The loop switch is a run state, so it is neither written nor read back.
    ///
    /// Persisting it meant a ticked box survived a restart and a port close,
    /// where it reads as "still looping" with nothing running.
    #[test]
    fn the_loop_switch_is_not_persisted() {
        let m = MultiSend {
            loop_enabled: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("loop_enabled"),
            "the run state must not be written: {json}"
        );

        let back: MultiSend = serde_json::from_str(&json).unwrap();
        assert!(!back.loop_enabled);

        // A file written before this change still carries the field; it must
        // not be able to resurrect a stale tick either.
        let legacy = json.replace("\"pages\"", "\"loop_enabled\":true,\"pages\"");
        let back: MultiSend = serde_json::from_str(&legacy).unwrap();
        assert!(!back.loop_enabled, "an old file must not restore a tick");
    }

    #[test]
    fn import_rejects_junk() {
        let mut m = MultiSend::default();
        assert!(m.import_json("not json").is_err());
        assert!(m.import_json("{}").is_err());
    }

    #[test]
    fn pages_grow_to_fill_the_panel_but_stay_bounded() {
        let mut m = MultiSend::default();
        assert!(m.ensure_len(0, 25), "growth is reported");
        assert_eq!(m.pages[0].len(), 25);
        // Growing is monotonic: a smaller request never discards entries the
        // user may already have filled in.
        assert!(!m.ensure_len(0, 2), "no growth, nothing to persist");
        assert_eq!(m.pages[0].len(), 25);
        assert!(m.ensure_len(0, MAX_ENTRIES_PER_PAGE + 500));
        assert_eq!(m.pages[0].len(), MAX_ENTRIES_PER_PAGE);
    }

    #[test]
    fn per_entry_delay_defaults_to_zero_and_is_reported() {
        let mut m = MultiSend::default();
        assert_eq!(m.delay_after(0, 0), 0);
        m.entry_mut(0, 3).expect("entry").delay_ms = 250;
        assert_eq!(m.delay_after(0, 3), 250);
    }

    #[test]
    fn normalize_repairs_a_short_page_table() {
        let mut m = MultiSend {
            pages: vec![vec![MultiEntry::default(); 2]],
            page: 99,
            ..Default::default()
        };
        m.normalize();
        assert_eq!(m.pages.len(), PAGES);
        assert_eq!(m.pages[0].len(), ENTRIES_PER_PAGE);
        assert_eq!(m.page, PAGES - 1);
    }

    #[test]
    fn loop_count_defaults_to_infinite_and_is_clamped() {
        assert_eq!(MultiSend::default().loop_count, None, "infinite by default");

        let mut m = MultiSend {
            loop_count: Some(0),
            ..Default::default()
        };
        m.normalize();
        assert_eq!(m.loop_count, Some(1), "zero would loop never");

        m.loop_count = Some(u32::MAX);
        m.normalize();
        assert_eq!(m.loop_count, Some(1_000_000));
    }

    #[test]
    fn newline_bytes_follow_flags() {
        let mut m = MultiSend {
            newline_cr: false,
            newline_lf: true,
            ..Default::default()
        };
        assert_eq!(m.newline_bytes(), vec![b'\n']);
        m.newline_cr = true;
        m.newline_lf = false;
        assert_eq!(m.newline_bytes(), vec![b'\r']);
        m.newline_cr = false;
        assert!(m.newline_bytes().is_empty());
    }
}
