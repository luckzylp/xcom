//! The multi-send panel: pages of preset commands with optional notes.
//!
//! The original tool offers four pages of ten entries, each optionally sent
//! with a newline and/or as hex, with the ability to loop through the checked
//! entries on a timer and to bind the first ten to the numeric keypad.

use serde::{Deserialize, Serialize};

/// Number of pages.
pub const PAGES: usize = 4;
/// Entries per page.
pub const ENTRIES_PER_PAGE: usize = 10;

/// One preset command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiEntry {
    /// The command text (hex form allowed when `hex` is set).
    pub text: String,
    /// Interpret `text` as hexadecimal bytes.
    pub hex: bool,
    /// Append CR/LF after the payload.
    pub newline: bool,
    /// Free-form description shown next to the entry.
    pub note: String,
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
pub struct MultiSend {
    /// `pages[page][index]`.
    pub pages: Vec<Vec<MultiEntry>>,
    /// Currently displayed page.
    pub page: usize,
    /// Loop through all enabled entries on a timer.
    pub loop_enabled: bool,
    pub loop_period_ms: u64,
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
    }

    /// Borrow one entry.
    pub fn entry(&self, page: usize, index: usize) -> Option<&MultiEntry> {
        self.pages.get(page)?.get(index)
    }

    /// Mutably borrow one entry.
    pub fn entry_mut(&mut self, page: usize, index: usize) -> Option<&mut MultiEntry> {
        self.pages.get_mut(page)?.get_mut(index)
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

    /// Indices of entries that the loop should send, in order.
    pub fn loop_indices(&self, page: usize) -> Vec<usize> {
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
        let doc: Doc = serde_json::from_str(text).map_err(|e| format!("解析失败: {e}"))?;
        if doc.pages.is_empty() {
            return Err("文件中没有条目".to_owned());
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
    fn loop_skips_disabled_and_empty_entries() {
        let mut m = MultiSend::default();
        for (index, (text, enabled)) in [("a", true), ("b", false), ("", true), ("c", true)]
            .into_iter()
            .enumerate()
        {
            let entry = m.entry_mut(0, index).expect("entry exists");
            entry.text = text.to_owned();
            entry.enabled = enabled;
        }
        assert_eq!(m.loop_indices(0), vec![0, 3]);
    }

    #[test]
    fn json_round_trip_preserves_entries() {
        let mut m = MultiSend::default();
        *m.entry_mut(0, 3).expect("entry exists") = MultiEntry {
            text: "0A0B".to_owned(),
            hex: true,
            newline: true,
            note: "备注".to_owned(),
            enabled: true,
        };
        let json = m.export_json().unwrap();
        let mut loaded = MultiSend::default();
        let count = loaded.import_json(&json).unwrap();
        assert_eq!(count, PAGES * ENTRIES_PER_PAGE);
        assert_eq!(loaded.pages[0][3], m.pages[0][3]);
    }

    #[test]
    fn import_rejects_junk() {
        let mut m = MultiSend::default();
        assert!(m.import_json("not json").is_err());
        assert!(m.import_json("{}").is_err());
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
